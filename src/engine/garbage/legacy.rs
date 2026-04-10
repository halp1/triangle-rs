use super::{
  GarbageQueue, GarbageQueueInitParams, GarbageQueueSnapshot, IncomingGarbage, OutgoingGarbage,
  RoundingMode,
};
use crate::engine::utils::rng::Rng;

fn column_width(board_width: usize, hole_size: usize) -> i32 {
  (board_width as i32 - (hole_size as i32 - 1)).max(0)
}

#[derive(Debug, Clone)]
pub struct LegacyGarbageQueue {
  pub options: GarbageQueueInitParams,
  pub queue: Vec<IncomingGarbage>,
  pub last_tank_time: i32,
  pub last_column: Option<i32>,
  pub rng: Rng,
  pub sent: i32,
}

impl LegacyGarbageQueue {
  pub fn new(mut options: GarbageQueueInitParams) -> Self {
    if options.cap.absolute == 0 {
      options.cap.absolute = i32::MAX;
    }
    let rng = Rng::new(options.seed);
    LegacyGarbageQueue {
      options,
      queue: Vec::new(),
      last_tank_time: 0,
      last_column: None,
      rng,
      sent: 0,
    }
  }

  fn rngex(&mut self) -> f64 {
    self.rng.next_float()
  }

  fn internal_reroll_column(&self, current: Option<i32>, rng: &mut Rng) -> i32 {
    let cols = column_width(self.options.board_width, self.options.garbage.hole_size);
    if self.options.messiness.nosame && current.is_some() {
      let lc = current.unwrap();
      let mut col = (rng.next_float() * (cols - 1) as f64) as i32;
      if col >= lc {
        col += 1;
      }
      col
    } else {
      (rng.next_float() * cols as f64) as i32
    }
  }

  fn reroll_column(&mut self) -> i32 {
    let lc = self.last_column;
    let col = {
      let rng = &mut self.rng;
      let cols = column_width(self.options.board_width, self.options.garbage.hole_size);
      if self.options.messiness.nosame && lc.is_some() {
        let c = lc.unwrap();
        let mut v = (rng.next_float() * (cols - 1) as f64) as i32;
        if v >= c {
          v += 1;
        }
        v
      } else {
        (rng.next_float() * cols as f64) as i32
      }
    };
    self.last_column = Some(col);
    col
  }

  pub fn size(&self) -> i32 {
    self.queue.iter().map(|g| g.amount).sum()
  }

  pub fn receive(&mut self, garbages: Vec<IncomingGarbage>) {
    for g in garbages {
      if g.amount > 0 {
        self.queue.push(g);
      }
    }

    let cap = self.options.cap.absolute;
    let mut total: i32 = self.queue.iter().map(|g| g.amount).sum();

    while total > cap && !self.queue.is_empty() {
      let excess = total - cap;
      let last = self.queue.last_mut().unwrap();
      if last.amount <= excess {
        total -= last.amount;
        self.queue.pop();
      } else {
        last.amount -= excess;
        total -= excess;
      }
    }
  }

  pub fn confirm(&mut self, cid: i32, gameid: i32, frame: i32) -> bool {
    if let Some(g) = self
      .queue
      .iter_mut()
      .find(|g| g.cid == cid && g.gameid == gameid)
    {
      g.frame = frame;
      g.confirmed = true;
      true
    } else {
      false
    }
  }

  pub fn cancel(
    &mut self,
    amount: i32,
    piece_count: i32,
    legacy_opener: bool,
  ) -> (i32, Vec<IncomingGarbage>) {
    let mut send = amount;
    let mut cancel = 0i32;

    let opener_phase = self.options.opener_phase;
    let current_size: i32 = self.queue.iter().map(|g| g.amount).sum();
    if piece_count + 1 <= opener_phase - (if legacy_opener { 1 } else { 0 })
      && current_size >= self.sent
    {
      cancel += amount;
    }

    let mut cancelled: Vec<IncomingGarbage> = Vec::new();

    while (send > 0 || cancel > 0) && !self.queue.is_empty() {
      self.queue[0].amount -= 1;

      let front_cid = self.queue[0].cid;
      if cancelled.is_empty()
        || cancelled.last().map(|c: &IncomingGarbage| c.cid) != Some(front_cid)
      {
        let mut entry = self.queue[0].clone();
        entry.amount = 1;
        cancelled.push(entry);
      } else {
        cancelled.last_mut().unwrap().amount += 1;
      }

      if self.queue[0].amount <= 0 {
        self.queue.remove(0);
      }

      if send > 0 {
        send -= 1;
      } else {
        cancel -= 1;
      }
    }

    self.sent += send;
    (send, cancelled)
  }

  fn internal_tank(
    options: &GarbageQueueInitParams,
    queue: &mut Vec<IncomingGarbage>,
    last_tank_time: &mut i32,
    last_column: &mut Option<i32>,
    rng: &mut Rng,
    frame: i32,
    cap: f64,
    hard: bool,
  ) -> Vec<OutgoingGarbage> {
    if queue.is_empty() {
      return vec![];
    }

    queue.sort_by_key(|g| g.frame);

    if options.messiness.timeout > 0 && frame >= *last_tank_time + options.messiness.timeout {
      let lc = *last_column;
      let cols = column_width(options.board_width, options.garbage.hole_size);
      let new_col = if options.messiness.nosame && lc.is_some() {
        let c = lc.unwrap();
        let mut v = (rng.next_float() * (cols - 1) as f64) as i32;
        if v >= c {
          v += 1;
        }
        v
      } else {
        (rng.next_float() * cols as f64) as i32
      };
      *last_column = Some(new_col);
      *last_tank_time = frame;
    }

    let mut total = 0i32;
    let max = cap.min(options.cap.max as f64).floor() as i32;
    let mut res: Vec<OutgoingGarbage> = Vec::new();

    while total < max && !queue.is_empty() {
      let item = queue[0].clone();
      let speed_threshold = if hard { frame } else { frame - 1 };
      if item.frame + options.garbage.speed > speed_threshold {
        break;
      }

      total += item.amount;
      let exhausted = total <= max;

      let mut remaining = item.amount;
      if total > max {
        let excess = total - max;
        queue[0].amount = excess;
        remaining -= excess;
        total = max;
      } else {
        queue.remove(0);
      }

      for _ in 0..remaining {
        let needs_reroll = last_column.is_none() || rng.next_float() < options.messiness.within;

        let col = if needs_reroll {
          let lc = *last_column;
          let cols = column_width(options.board_width, options.garbage.hole_size);
          let new_col = if options.messiness.nosame && lc.is_some() {
            let c = lc.unwrap();
            let mut v = (rng.next_float() * (cols - 1) as f64) as i32;
            if v >= c {
              v += 1;
            }
            v
          } else {
            (rng.next_float() * cols as f64) as i32
          };
          *last_column = Some(new_col);
          new_col
        } else {
          last_column.unwrap_or(0)
        };

        res.push(OutgoingGarbage {
          frame: item.frame,
          amount: 1,
          size: item.size,
          id: item.cid,
          column: col as usize,
        });
      }

      if exhausted && rng.next_float() < options.messiness.change {
        let lc = *last_column;
        let cols = column_width(options.board_width, options.garbage.hole_size);
        let new_col = if options.messiness.nosame && lc.is_some() {
          let c = lc.unwrap();
          let mut v = (rng.next_float() * (cols - 1) as f64) as i32;
          if v >= c {
            v += 1;
          }
          v
        } else {
          (rng.next_float() * cols as f64) as i32
        };
        *last_column = Some(new_col);
      }
    }

    res
  }

  pub fn predict(&self) -> Vec<OutgoingGarbage> {
    let mut rng = self.rng.clone();
    let mut queue = self.queue.clone();
    let mut last_tank_time = self.last_tank_time;
    let mut last_column = self.last_column;
    Self::internal_tank(
      &self.options,
      &mut queue,
      &mut last_tank_time,
      &mut last_column,
      &mut rng,
      i32::MIN / 2,
      i32::MAX as f64,
      false,
    )
  }

  pub fn next_column(&self) -> i32 {
    if self.last_column.is_none() {
      let mut rng = self.rng.clone();
      self.internal_reroll_column(None, &mut rng)
    } else {
      self.last_column.unwrap()
    }
  }

  pub fn tank(&mut self, frame: i32, cap: f64, hard: bool) -> Vec<OutgoingGarbage> {
    let res = Self::internal_tank(
      &self.options,
      &mut self.queue,
      &mut self.last_tank_time,
      &mut self.last_column,
      &mut self.rng,
      frame,
      cap,
      hard,
    );
    res
  }

  pub fn round(&mut self, amount: f64) -> i32 {
    match self.options.rounding {
      RoundingMode::Down => amount.floor() as i32,
      RoundingMode::Rng => {
        let floored = amount.floor() as i32;
        if amount.fract() == 0.0 {
          floored
        } else {
          let decimal = amount - floored as f64;
          floored
            + if self.rng.next_float() < decimal {
              1
            } else {
              0
            }
        }
      }
    }
  }

  pub fn reset(&mut self) {
    self.queue.clear();
  }

  pub fn snapshot(&self) -> GarbageQueueSnapshot {
    GarbageQueueSnapshot {
      seed: self.rng.seed(),
      last_tank_time: self.last_tank_time,
      last_column: self.last_column,
      sent: self.sent,
      has_changed_column: false,
      last_received_count: 0,
      queue: self.queue.clone(),
    }
  }

  pub fn from_snapshot(&mut self, snapshot: &GarbageQueueSnapshot) {
    self.queue = snapshot.queue.clone();
    self.last_tank_time = snapshot.last_tank_time;
    self.last_column = snapshot.last_column;
    self.rng = Rng::new(snapshot.seed);
    self.sent = snapshot.sent;
  }
}
