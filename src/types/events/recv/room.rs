use crate::macros::event;
use crate::types::{game, room, user};
use serde_json::Value;

event!(room.join => Join {
  id: String,
  banner: Option<Value>,
  silent: bool,
});

event!(room.leave => Leave(String));

event!(room.kick => Kick(String));

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Update {
  pub id: String,
  pub public: bool,
  pub name: String,
  pub name_safe: Option<String>,
  #[serde(rename = "type")]
  pub r#type: room::Type,
  pub owner: String,
  pub creator: String,
  #[serde(rename = "allowChat")]
  pub allow_chat: Option<bool>,
  #[serde(rename = "userLimit")]
  pub user_limit: u32,
  #[serde(rename = "autoStart")]
  pub auto_start: u32,
  pub state: room::State,
  pub topic: Value,
  pub info: Value,
  pub auto: room::Autostart,
  #[serde(rename = "allowAnonymous")]
  pub allow_anonymous: bool,
  #[serde(rename = "allowUnranked")]
  pub allow_unranked: bool,
  #[serde(rename = "allowQueued")]
  pub allow_queued: bool,
  #[serde(rename = "allowBots")]
  pub allow_bots: bool,
  #[serde(rename = "userRankLimit")]
  pub user_rank_limit: game::Rank,
  #[serde(rename = "useBestRankAsLimit")]
  pub use_best_rank_as_limit: bool,
  pub options: Option<game::Options>,
  #[serde(rename = "match")]
  pub r#match: room::Match,
  pub players: Vec<room::Player>,
  pub lobbybg: Option<String>,
  pub lobbybgm: String,
  pub gamebgm: String,
  #[serde(rename = "forceRequireXPToChat")]
  pub force_require_xp_to_chat: bool,
  #[serde(rename = "bgmList")]
  pub bgm_list: Vec<Value>,
  pub constants: Value,
}

impl crate::utils::events::Event for Update {
  const NAME: &'static str = "room.update";
}

pub mod update {
  use super::*;

  event!(room.update.auto => Auto {
    enabled: bool,
    status: String,
    time: f64,
    maxtime: f64,
  });

  event!(room.update.host => Host(String));

  event!(room.update.bracket => Bracket {
    uid: String,
    bracket: room::Bracket,
  });
}

pub mod player {
  use super::*;

  event!(room.player.add => Add(room::Player));

  event!(room.player.remove => Remove(String));
}

event!(room.chat => Chat {
  content: String,
  content_safe: Option<String>,
  suppressable: Option<bool>,
  user: room::ChatUser,
  pinned: Option<bool>,
  system: bool,
});

pub mod chat {
  use super::*;

  event!(room.chat.delete => Delete {
    uid: String,
    purge: String,
  });

  event!(room.chat.clear => Clear);

  event!(room.chat.gift => Gift {
    sender: u32,
    target: u32,
    months: u32,
  });
}
