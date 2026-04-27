use crate::macros::event;
use crate::types::social;

event!(social.online => Online(u32));
event!(social.dm => DM = social::dm::DM);

pub mod dm {
  use super::*;

  event!(social.dm.fail => Fail(social::dm::FailReason));
}

event!(social.presence => Presence {
  user: String,
  presence: social::Presence,
});

event!(social.relationship.remove => Remove(String));
event!(social.relationship.add => Add = social::relationship::Relationship);
event!(social.notification => Notification = social::Notification);
event!(social.invite => Invite {
	sender: String,
	roomid: String,
	roomname: String,
	roomname_safe: String,
});
