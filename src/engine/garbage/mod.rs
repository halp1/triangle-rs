pub mod legacy;

use crate::engine::utils::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarbageQueueInitParams {
  pub cap: GarbageCapParams,
  pub messiness: MessinessParams,
  pub garbage: GarbageSpeedParams,
  pub multiplier: MultiplierParams,
  pub bombs: bool,
  pub seed: i64,
  pub board_width: usize,
  pub rounding: RoundingMode,
  pub opener_phase: i32,
  pub special_bonus: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarbageCapParams {
  pub value: f64,
  pub margin_time: i32,
  pub increase: f64,
  pub absolute: i32,
  pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessinessParams {
  pub change: f64,
  pub within: f64,
  pub nosame: bool,
  pub timeout: i32,
  pub center: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarbageSpeedParams {
  pub speed: i32,
  pub hole_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiplierParams {
  pub value: f64,
  pub increase: f64,
  pub margin_time: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoundingMode {
  Down,
  Rng,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingGarbage {
  pub frame: i32,
  pub amount: i32,
  pub size: usize,
  pub cid: i32,
  pub gameid: i32,
  pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingGarbage {
  pub frame: i32,
  pub amount: i32,
  pub size: usize,
  pub id: i32,
  pub column: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarbageQueueSnapshot {
  pub seed: i64,
  pub last_tank_time: i32,
  pub last_column: Option<i32>,
  pub sent: i32,
  pub has_changed_column: bool,
  pub last_received_count: i32,
  pub queue: Vec<IncomingGarbage>,
}

#[derive(Debug, Clone)]
pub struct GarbageQueue {
  pub options: GarbageQueueInitParams,
  pub queue: Vec<IncomingGarbage>,
  pub last_tank_time: i32,
  pub last_column: Option<i32>,
  pub has_changed_column: bool,
  pub last_received_count: i32,
  pub rng: Rng,
  pub sent: i32,
}

impl GarbageQueue {
  pub fn new(mut options: GarbageQueueInitParams) -> Self {
    if options.cap.absolute == 0 {
      options.cap.absolute = i32::MAX;
    }
    let rng = Rng::new(options.seed);
    GarbageQueue {
      options,
      queue: Vec::new(),
      last_tank_time: 0,
      last_column: None,
      has_changed_column: false,
      last_received_count: 0,
      rng,
      sent: 0,
    }
  }

  fn rngex(&mut self) -> f64 {
    self.rng.next_float()
  }

  fn column_width(&self) -> i32 {
    (self.options.board_width as i32 - (self.options.garbage.hole_size as i32 - 1)).max(0)
  }

  fn reroll_column(&mut self) -> i32 {
    let center_buffer = if self.options.messiness.center {
      (self.options.board_width as f64 / 5.0).round() as i32
    } else {
      0
    };

    let col = if self.options.messiness.nosame && self.last_column.is_some() {
      let lc = self.last_column.unwrap();
      let range = (self.column_width() - 1 - 2 * center_buffer).max(0);
      let mut c = center_buffer + (self.rngex() * range as f64) as i32;
      if c >= lc {
        c += 1;
      }
      c
    } else {
      let range = (self.column_width() - 2 * center_buffer).max(0);
      center_buffer + (self.rngex() * range as f64) as i32
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
    let mut current_size = current_size;

    while (send > 0 || cancel > 0) && !self.queue.is_empty() {
      self.queue[0].amount -= 1;
      current_size -= 1;

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
        if self.rngex() < self.options.messiness.change {
          self.reroll_column();
          self.has_changed_column = true;
        }
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

  pub fn tank(&mut self, frame: i32, cap: f64, hard: bool) -> Vec<OutgoingGarbage> {
    if self.queue.is_empty() {
      return vec![];
    }

    self.queue.sort_by_key(|g| g.frame);

    if self.options.messiness.timeout > 0
      && frame >= self.last_tank_time + self.options.messiness.timeout
    {
      self.reroll_column();
      self.has_changed_column = true;
    }

    let lines = cap.min(self.options.cap.max as f64).floor() as i32;
    let mut res: Vec<OutgoingGarbage> = Vec::new();

    let mut i = 0;
    while i < lines && !self.queue.is_empty() {
      let item = &self.queue[0];
      let speed_threshold = if hard { frame } else { frame - 1 };
      if item.frame + self.options.garbage.speed > speed_threshold {
        break;
      }

      let mut item = self.queue[0].clone();
      item.amount -= 1;
      self.queue[0].amount -= 1;
      self.last_received_count += 1;

      let col = if self.last_column.is_none()
        || (self.rngex() < self.options.messiness.within && !self.has_changed_column)
      {
        let c = self.reroll_column();
        self.has_changed_column = true;
        c
      } else {
        self.last_column.unwrap_or(0)
      };

      res.push(OutgoingGarbage {
        frame: item.frame,
        amount: 1,
        size: item.size,
        id: item.cid,
        column: col as usize,
      });

      self.has_changed_column = false;

      if self.queue[0].amount <= 0 {
        self.queue.remove(0);
        if self.rngex() < self.options.messiness.change {
          self.reroll_column();
          self.has_changed_column = true;
        }
      }

      i += 1;
    }

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
          floored + if self.rngex() < decimal { 1 } else { 0 }
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
      has_changed_column: self.has_changed_column,
      last_received_count: self.last_received_count,
      queue: self.queue.clone(),
    }
  }

  pub fn from_snapshot(&mut self, snapshot: &GarbageQueueSnapshot) {
    self.queue = snapshot.queue.clone();
    self.last_tank_time = snapshot.last_tank_time;
    self.last_column = snapshot.last_column;
    self.rng = Rng::new(snapshot.seed);
    self.sent = snapshot.sent;
    self.has_changed_column = snapshot.has_changed_column;
    self.last_received_count = snapshot.last_received_count;
  }
}
