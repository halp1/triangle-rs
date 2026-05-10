use serde_json::Value;

use crate::{
  macros::event,
  types::game::{self, GameMode, Leaderboard, MatchData, Scoreboard},
  utils::events::Event,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Ready {
  pub players: Vec<game::ReadyPlayer>,
  #[serde(rename = "isNew")]
  pub is_new: bool,
}

impl Event for Ready {
  const NAME: &'static str = "game.ready";
}

event!(game.abort => Abort);

event!(game.match => Match {
  gamemode: GameMode,
  modename: String,
  rb: Value,
  rrb: Value,
});

event!(game.start => Start);

event!(game.over => Over {
  winner: Option<String>,
  reason: String,
});

event!(game.advance => Advance {
  scoreboard: Value
});

event!(game.score => Score {
  scoreboard: Vec<Scoreboard>,
  r#match: MatchData
});

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct End {
  pub leaderboard: Option<Vec<Leaderboard>>,
  pub scoreboard: Option<Vec<Scoreboard>>,
  #[serde(rename = "xpPerUser")]
  pub xp_per_user: f64,
  pub winners: Vec<Value>,
}

impl Event for End {
  const NAME: &'static str = "game.end";
}

pub mod replay {
  use crate::types::game::ReplayState;

  use super::*;

  event!(game.replay.state => State {
    gameid: u64,
    data: ReplayState
  });

  event!(game.replay.ige => IGE {
    gameid: u64,
    data: Vec<game::ige::IGE>
  });

  event!(game.replay.board => Board {
    // TODO: type (not very important right now)
    boards: Vec<Value>
  });

  event!(game.replay.end => End {
    gameid: u64,
    data: game::ReplayEndData
  });
}

event!(game.spectate => Spectate {
  // TODO: type
  players: Value,
  r#match: MatchData
});
event!(game.replay => Replay {
  gameid: u64,
  provisioned: u64,
  frames: Vec<game::replay::Frame>,
});
