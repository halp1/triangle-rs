use crate::{macros::event, types::social};

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

// TODO: client.room.players

// TODO: client.room.join

pub mod room {
  pub use super::*;
  event!(client.room.join => Join);
}

pub mod game {
  pub use super::*;
  // TODO: client.game.start
  // TODO: client.game.over
  pub mod round {
    pub use super::*;
    // TODO: client.game.round.start
    event!(client.game.round.end => End(Option<String>));
  }

  event!(client.game.abort => Abort);
}

pub mod ribbon {
  pub use super::*;

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
  relationship: social::relationship::Relationship,
  raw: social::dm::DM,
  content: String
  // TODO: reply?
});