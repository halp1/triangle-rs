use crate::{classes::social::relationship::Relationship, macros::event, types::social};

event!(client.ready => Ready {
  endpoint: String,
  social: social::Summary,
});

event!(client.fail => Fail(String));

event!(client.error => Error(String));

event!(client.dead => Dead(String));

event!(client.close => Close {
  reason: String,
});

event!(client.notify => Notify(String));

pub mod room {
  use super::*;

  event!(client.room.players => Players(Vec<crate::types::room::Player>));
  event!(client.room.join => Join);
}

pub mod game {
  use serde::{Deserialize, Serialize};

  use crate::{types::game::ReplayEndData, utils::events::Event};

  use super::*;

  // players: (id, username)
  event!(client.game.start => Start {
    multi: bool,
    first_to: u32,
    win_by: u32,
    golden_point: u32,
    players: Vec<(String, String)>,
  });

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub enum Over {
    End,
    Leave,
    Abort,
    Finish(ReplayEndData),
  }

  impl Event for Over {
    const NAME: &'static str = "client.game.over";
  }

  pub mod round {
    use super::*;

    event!(client.game.round.start => Start {});
    event!(client.game.round.end => End(Option<String>));
  }

  event!(client.game.abort => Abort);

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct EndPlayer {
    pub id: String,
    pub name: String,
    pub points: i64,
    pub won: bool,
    pub lifetime: Option<i64>,
    pub raw: serde_json::Value,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(rename_all = "lowercase")]
  pub enum EndSource {
    Scoreboard,
    Leaderboard,
  }

  event!(client.game.end => End {
    duration_ms: f64,
    source: EndSource,
    players: Vec<EndPlayer>,
  });
}

pub mod ribbon {
  use super::*;

  event!(client.ribbon.receive => Receive {
    command: String,
    data: serde_json::Value,
  });

  event!(client.ribbon.send => Send {
    command: String,
    data: serde_json::Value,
  });

  event!(client.ribbon.log => Log(String));
  event!(client.ribbon.warn => Warn(String));
  event!(client.ribbon.error => Error(String));
}

event!(client.friended => Friended {
  id: String,
  name: String,
  avatar: Option<u64>,
});

event!(client.dm => DM {
  user_id: String,
  username: String,
  raw: social::dm::DM,
  content: String
});
