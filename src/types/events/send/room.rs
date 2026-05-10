use crate::macros::event;

event!(room.leave => Leave);
event!(room.kick => Kick {
  uid: String,
  duration: f64,
});

event!(room.unban => Unban(String));

event!(room.chat => Chat {
  content: String,
  pinned: bool
});

pub mod chat {
  use crate::macros::event;

  event!(room.chat.clear => Clear);
}

event!(room.setid => SetId(String));

event!(room.setconfig => SetConfig(Vec<crate::types::room::SetConfigItemRaw>));

pub mod bracket {
  use crate::{macros::event, types::room::Bracket};

  event!(room.bracket.switch => Switch(Bracket));

  event!(room.bracket.move => Move {
    uid: String,
    bracket: Bracket,
  });
}

pub mod owner {
  use crate::macros::event;

  event!(room.owner.transfer => Transfer(String));
  event!(room.owner.revoke => Revoke);
}

event!(room.start => Start);
event!(room.abort => Abort);
