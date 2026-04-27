use serde_json::{Value, json};

use crate::{
  error::{Result, TriangleError},
  types::{
    game::Options as GameOptions,
    room::{Autostart, Match, Player, Preset, SetConfigItem, State, Type},
  },
};

use super::{
  client::Client,
  game::{Game, parse_raw_players},
};

#[derive(Clone)]
pub struct Room {
  pub id: String,
  pub public: bool,
  pub room_type: Type,
  pub name: String,
  pub name_safe: String,
  pub owner: String,
  pub creator: String,
  pub state: State,
  pub auto: Autostart,
  pub match_config: Match,
  pub players: Vec<Player>,
  pub user_limit: u32,
  pub allow_chat: bool,
  pub allow_anonymous: bool,
  pub allow_unranked: bool,
  pub allow_queued: bool,
  pub allow_bots: bool,
  pub user_rank_limit: crate::types::game::Rank,
  pub use_best_rank_as_limit: bool,
  pub lobbybg: Option<String>,
  pub lobbybgm: String,
  pub gamebgm: String,
  pub force_require_xp_to_chat: bool,
  pub options: GameOptions,
  pub game_start: Option<i64>,
  pub chats: Vec<Value>,
}

impl Room {
  pub fn from_update(data: &Value) -> Option<Self> {
    let id = data["id"].as_str()?.to_string();
    let state: State = serde_json::from_value(data["state"].clone()).ok()?;
    let room_type: Type = serde_json::from_value(data["type"].clone()).unwrap_or_default();
    let auto: Autostart = serde_json::from_value(data["auto"].clone()).unwrap_or(Autostart {
      enabled: false,
      status: String::new(),
      time: 0.0,
      maxtime: 0.0,
    });
    let match_config: Match = serde_json::from_value(data["match"].clone()).ok()?;
    let players: Vec<Player> = data["players"]
      .as_array()
      .map(|arr| {
        arr
          .iter()
          .filter_map(|v| serde_json::from_value(v.clone()).ok())
          .collect()
      })
      .unwrap_or_default();
    let options: GameOptions = serde_json::from_value(data["options"].clone()).unwrap_or_default();

    Some(Self {
      id,
      public: data["public"].as_bool().unwrap_or(false),
      room_type,
      name: data["name"].as_str().unwrap_or("").to_string(),
      name_safe: data["name_safe"].as_str().unwrap_or("").to_string(),
      owner: data["owner"].as_str().unwrap_or("").to_string(),
      creator: data["creator"].as_str().unwrap_or("").to_string(),
      state,
      auto,
      match_config,
      players,
      user_limit: data["userLimit"].as_u64().unwrap_or(0) as u32,
      allow_chat: data["allowChat"].as_bool().unwrap_or(true),
      allow_anonymous: data["allowAnonymous"].as_bool().unwrap_or(false),
      allow_unranked: data["allowUnranked"].as_bool().unwrap_or(true),
      allow_queued: data["allowQueued"].as_bool().unwrap_or(true),
      allow_bots: data["allowBots"].as_bool().unwrap_or(false),
      user_rank_limit: serde_json::from_value(data["userRankLimit"].clone()).unwrap_or_default(),
      use_best_rank_as_limit: data["useBestRankAsLimit"].as_bool().unwrap_or(false),
      lobbybg: data["lobbybg"].as_str().map(|s| s.to_string()),
      lobbybgm: data["lobbybgm"].as_str().unwrap_or("").to_string(),
      gamebgm: data["gamebgm"].as_str().unwrap_or("").to_string(),
      force_require_xp_to_chat: data["forceRequireXPToChat"].as_bool().unwrap_or(false),
      options,
      game_start: None,
      chats: Vec::new(),
    })
  }

  pub fn apply_update(&mut self, data: &Value) {
    if let Ok(s) = serde_json::from_value::<State>(data["state"].clone()) {
      self.state = s;
    }
    if let Ok(t) = serde_json::from_value::<Type>(data["type"].clone()) {
      self.room_type = t;
    }
    if let Some(s) = data["name"].as_str() {
      self.name = s.to_string();
    }
    if let Some(s) = data["name_safe"].as_str() {
      self.name_safe = s.to_string();
    }
    if let Some(s) = data["owner"].as_str() {
      self.owner = s.to_string();
    }
    if let Ok(auto) = serde_json::from_value::<Autostart>(data["auto"].clone()) {
      self.auto = auto;
    }
    if let Ok(players) = serde_json::from_value::<Vec<Player>>(data["players"].clone()) {
      self.players = players;
    }
    if let Ok(opts) = serde_json::from_value::<GameOptions>(data["options"].clone()) {
      merge_options(&mut self.options, opts);
    }
  }

  pub fn snapshot(&self) -> Value {
    json!({
      "id": self.id,
      "public": self.public,
      "type": self.room_type,
      "name": self.name,
      "name_safe": self.name_safe,
      "owner": self.owner,
      "creator": self.creator,
      "state": self.state,
      "auto": self.auto,
      "match": self.match_config,
      "players": self.players,
      "userLimit": self.user_limit,
      "allowChat": self.allow_chat,
      "allowAnonymous": self.allow_anonymous,
      "allowUnranked": self.allow_unranked,
      "allowQueued": self.allow_queued,
      "allowBots": self.allow_bots,
      "userRankLimit": self.user_rank_limit,
      "useBestRankAsLimit": self.use_best_rank_as_limit,
      "lobbybg": self.lobbybg,
      "lobbybgm": self.lobbybgm,
      "gamebgm": self.gamebgm,
      "forceRequireXPToChat": self.force_require_xp_to_chat,
      "options": self.options,
      "game_start": self.game_start,
      "chats": self.chats,
    })
  }

  pub fn is_host(&self, user_id: &str) -> bool {
    self.owner == user_id
  }

  pub fn self_player(&self, user_id: &str) -> Option<&Player> {
    self.players.iter().find(|p| p.id == user_id)
  }

  pub async fn leave(&self, client: &Client) -> Result<()> {
    let _ = client.wrap("room.leave", Value::Null, "room.leave").await?;
    Ok(())
  }

  pub async fn kick(&self, client: &Client, user_id: &str, duration: u32) -> Result<Value> {
    client
      .wrap(
        "room.kick",
        json!({ "uid": user_id, "duration": duration }),
        "room.player.remove",
      )
      .await
  }

  pub async fn ban(&self, client: &Client, user_id: &str) -> Result<Value> {
    self.kick(client, user_id, 2_592_000).await
  }

  pub fn unban(&self, client: &Client, username: &str) {
    client.emit("room.unban", Value::String(username.to_string()));
  }

  pub async fn chat(&self, client: &Client, message: &str, pinned: bool) -> Result<Value> {
    client
      .wrap(
        "room.chat.send",
        json!({ "content": message, "pinned": pinned }),
        "room.chat",
      )
      .await
  }

  pub async fn clear_chat(&self, client: &Client) -> Result<Value> {
    client
      .wrap("room.chat.clear", Value::Null, "room.chat.clear")
      .await
  }

  pub async fn set_id(&self, client: &Client, id: &str) -> Result<Value> {
    client
      .wrap(
        "room.setid",
        Value::String(id.to_uppercase()),
        "room.update",
      )
      .await
  }

  pub async fn update(&self, client: &Client, options: &[SetConfigItem]) -> Result<Value> {
    let payload: Vec<Value> = options
      .iter()
      .map(|opt| json!({ "index": opt.index, "value": opt.value }))
      .collect();
    client
      .wrap("room.setconfig", Value::Array(payload), "room.update")
      .await
  }

  pub async fn use_preset(&self, client: &Client, preset: Preset) -> Result<Value> {
    let mut options = room_preset_base(preset);
    options.push(SetConfigItem {
      index: "options.presets".to_string(),
      value: Value::String(preset.as_str().to_string()),
    });
    self.update(client, &options).await
  }

  pub async fn start(&self, client: &Client) -> Result<Value> {
    client.wrap("room.start", Value::Null, "game.ready").await
  }

  pub async fn abort(&self, client: &Client) -> Result<Value> {
    client.wrap("room.abort", Value::Null, "game.abort").await
  }

  pub async fn spectate(&self, client: &mut Client) -> Result<Value> {
    let spectate_data = client
      .wrap("game.spectate", Value::Null, "game.spectate")
      .await?;
    let players = parse_raw_players(&spectate_data);
    if players.is_empty() {
      return Err(TriangleError::Adapter(
        "game.spectate did not include players".to_string(),
      ));
    }
    let strategy = client.spectating_strategy().clone();
    client.game = Some(Game::new(
      client.ribbon.emitter.clone(),
      players,
      &client.user.id,
      strategy,
      client.ribbon.make_send_fn(),
    ));
    Ok(spectate_data)
  }

  pub fn unspectate(&self, client: &mut Client) {
    if let Some(game) = &client.game {
      game.unspectate_all();
    }
    client.game = None;
  }

  pub async fn transfer_host(&self, client: &Client, player: &str) -> Result<Value> {
    client
      .wrap(
        "room.owner.transfer",
        Value::String(player.to_string()),
        "room.update.host",
      )
      .await
  }

  pub async fn take_host(&self, client: &Client) -> Result<Value> {
    client
      .wrap("room.owner.revoke", Value::Null, "room.update.host")
      .await
  }

  pub async fn switch_bracket(&self, client: &Client, bracket: &str) -> Result<Value> {
    client
      .wrap(
        "room.bracket.switch",
        Value::String(bracket.to_string()),
        "room.update.bracket",
      )
      .await
  }

  pub async fn move_bracket(&self, client: &Client, uid: &str, bracket: &str) -> Result<Value> {
    client
      .wrap(
        "room.bracket.move",
        json!({ "uid": uid, "bracket": bracket }),
        "room.update.bracket",
      )
      .await
  }
}

fn room_preset_base(preset: Preset) -> Vec<SetConfigItem> {
  let mut options = vec![
    SetConfigItem {
      index: "match.gamemode".to_string(),
      value: Value::String("versus".to_string()),
    },
    SetConfigItem {
      index: "options.stock".to_string(),
      value: json!(0),
    },
    SetConfigItem {
      index: "options.boardwidth".to_string(),
      value: json!(10),
    },
    SetConfigItem {
      index: "options.boardheight".to_string(),
      value: json!(20),
    },
    SetConfigItem {
      index: "options.usebombs".to_string(),
      value: json!(false),
    },
  ];

  match preset {
    Preset::Default => {
      options.extend([
        SetConfigItem {
          index: "match.modename".to_string(),
          value: Value::String("VERSUS".to_string()),
        },
        SetConfigItem {
          index: "match.ft".to_string(),
          value: json!(1),
        },
        SetConfigItem {
          index: "options.spinbonuses".to_string(),
          value: Value::String("T-spins".to_string()),
        },
      ]);
    }
    Preset::TetraLeagueSeason1 => {
      options.extend([
        SetConfigItem {
          index: "match.modename".to_string(),
          value: Value::String("TETRA LEAGUE".to_string()),
        },
        SetConfigItem {
          index: "match.ft".to_string(),
          value: json!(7),
        },
        SetConfigItem {
          index: "options.roundmode".to_string(),
          value: Value::String("down".to_string()),
        },
      ]);
    }
    Preset::TetraLeague => {
      options.extend([
        SetConfigItem {
          index: "match.modename".to_string(),
          value: Value::String("TETRA LEAGUE".to_string()),
        },
        SetConfigItem {
          index: "match.ft".to_string(),
          value: json!(7),
        },
        SetConfigItem {
          index: "options.spinbonuses".to_string(),
          value: Value::String("all-mini+".to_string()),
        },
      ]);
    }
    Preset::Classic => {
      options.extend([
        SetConfigItem {
          index: "match.modename".to_string(),
          value: Value::String("CLASSIC VERSUS".to_string()),
        },
        SetConfigItem {
          index: "options.bagtype".to_string(),
          value: Value::String("classic".to_string()),
        },
        SetConfigItem {
          index: "options.spinbonuses".to_string(),
          value: Value::String("none".to_string()),
        },
      ]);
    }
    Preset::EnforcedDelays => {
      options.extend([
        SetConfigItem {
          index: "match.modename".to_string(),
          value: Value::String("VERSUS".to_string()),
        },
        SetConfigItem {
          index: "match.ft".to_string(),
          value: json!(2),
        },
        SetConfigItem {
          index: "options.garbageblocking".to_string(),
          value: Value::String("limited blocking".to_string()),
        },
      ]);
    }
    Preset::Arcade => {
      options.extend([
        SetConfigItem {
          index: "match.modename".to_string(),
          value: Value::String("ARCADE VERSUS".to_string()),
        },
        SetConfigItem {
          index: "options.display_hold".to_string(),
          value: json!(false),
        },
        SetConfigItem {
          index: "options.nextcount".to_string(),
          value: json!(1),
        },
      ]);
    }
  }

  options
}

fn merge_options(dst: &mut GameOptions, src: GameOptions) {
  macro_rules! assign_some {
    ($field:ident) => {
      if src.$field.is_some() {
        dst.$field = src.$field;
      }
    };
  }
  assign_some!(version);
  assign_some!(seed_random);
  assign_some!(seed);
  assign_some!(g);
  assign_some!(stock);
  assign_some!(countdown);
  assign_some!(hasgarbage);
  assign_some!(garbageentry);
  assign_some!(garbageblocking);
  assign_some!(spinbonuses);
  assign_some!(combotable);
  assign_some!(kickset);
  assign_some!(nextcount);
  assign_some!(boardwidth);
  assign_some!(boardheight);
  assign_some!(handling);
  assign_some!(username);
  assign_some!(gravitymay20g);
  assign_some!(messiness_center);
}
