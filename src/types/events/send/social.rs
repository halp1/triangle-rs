use crate::{macros::event, types::social};

event!(social.presence => Presence {
  status: social::Status,
  detail: social::Detail,
});

event!(social.dm => DM {
  recipient: String,
  msg: String
});

event!(social.invite => Invite(String));

pub mod notification {
  pub use super::*;

  event!(social.notification.ack => Ack);
}

pub mod relation {
  pub use super::*;
  event!(social.relation.ack => Ack);
}
