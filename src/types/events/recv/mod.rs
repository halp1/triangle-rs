use serde_json::Value;

use crate::macros::event;

pub mod client;
pub mod game;
pub mod room;
pub mod root;
pub mod server;
pub mod social;
pub mod staff;

event!(session => Session {
  ribbonid: String,
  tokenid: String
});

event!(ping => Ping {
  recvid: u64,
});

event!(kick => Kick {
  reason: String,
});

event!(nope => Nope {
  reason: String,
});

event!(notify => Notify(Value));

// TODO: packets
