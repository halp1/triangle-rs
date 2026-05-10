use crate::{macros::event, types::game::replay::Frame};

pub mod scope {
  use super::*;

  event!(game.scope.start => Start(u64));
  event!(game.scope.end => End(u64));
}

event!(game.spectate => Spectate);

event!(game.replay => Replay {
  gameid: u64,
  provisioned: u64,
  frames: Vec<Frame>,
});
