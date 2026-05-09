use crate::engine::queue::Mino;
use crate::macros::event;

pub mod queue {

  use super::*;
  event!(queue.add => Add(Vec<Mino>));
}

pub mod garbage {
  use super::*;

  event!(garbage.receive => Receive {
    iid: u64,
    amount: u32,
    original_amount: u32,
  });
  event!(garbage.confirm => Confirm {
    iid: u64,
    gameid: u64,
    frame: u64,
  });
  event!(garbage.tank => Tank {
    iid: u64,
    column: usize,
    amount: u32,
    size: usize,
  });
  event!(garbage.cancel => Cancel {
    iid: u64,
    amount: u32,
    size: usize,
  });
}

pub mod falling {
  use crate::engine::LockResult;

  use super::*;

  event!(falling.new => New {
    piece: Mino,
    is_hold: bool,
  });
  event!(falling.lock_pre => LockPre);
  event!(falling.lock => Lock(LockResult));
}
