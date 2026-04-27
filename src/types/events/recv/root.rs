use crate::macros::event;
use serde_json::Value;

event!(session => Session {
  ribbonid: String,
  tokenid: String,
});

event!(ping => Ping {
  recvid: u32,
});

event!(kick => Kick {
  reason: String,
});

event!(nope => Nope {
  reason: String,
});

event!(rejected => Rejected);

event!(error => Error(Value));

event!(err => Err(Value));

event!(packets => Packets {
  packets: Vec<Value>,
});

event!(notify => Notify(Value));
