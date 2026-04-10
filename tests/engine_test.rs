use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;
use triangle::engine::{
  AllowedOptions, B2bCharging, B2bOptions, Engine, EngineInitParams, GameOptions, HandlingOptions,
  IgeData, IgeFrame, IncreasableValue, KeyEvent, MiscOptions, MovementOptions, MultiplayerOptions,
  PcOptions, ReplayFrame,
  board::BoardInitParams,
  garbage::{
    GarbageCapParams, GarbageQueueInitParams, GarbageSpeedParams, MessinessParams,
    MultiplierParams, RoundingMode,
  },
  queue::{QueueInitParams, bag::BagType},
  utils::kicks::legal,
};

struct ReplayRound {
  id: String,
  index: usize,
  config: EngineInitParams,
  frames: Vec<Value>,
}

#[test]
fn replay_test() {
  let replay_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/replays");
  let files = replay_files(&replay_dir);

  assert!(
    !files.is_empty(),
    "No replays found. Use `make copy-test-data` or `make download-test-data` first."
  );

  for file in files {
    let file_text = fs::read_to_string(&file)
      .unwrap_or_else(|e| panic!("Failed to read replay file {}: {e}", file.display()));
    let replay_doc: Value = serde_json::from_str(&file_text)
      .unwrap_or_else(|e| panic!("Failed to parse replay file {}: {e}", file.display()));

    let replay_id = file
      .file_stem()
      .and_then(|v| v.to_str())
      .unwrap_or("<unknown>")
      .to_string();
    let user_id = replay_doc
      .pointer("/user/id")
      .and_then(Value::as_str)
      .unwrap_or_default()
      .to_string();
    let date = parse_replay_date(&replay_doc);

    let rounds = find_rounds(&replay_doc, &user_id)
      .into_iter()
      .enumerate()
      .map(|(idx, round)| ReplayRound {
        id: replay_id.clone(),
        index: idx,
        config: convert_round(&round.player, &round.opponents, date),
        frames: round.frames,
      })
      .collect::<Vec<_>>();

    for round in rounds {
      assert!(
        run_through(&round),
        "Failure at: https://tetr.io/#R:{}@{}",
        round.id,
        round.index + 1
      );
    }
  }
}

struct FoundRound {
  player: Value,
  opponents: Vec<i32>,
  frames: Vec<Value>,
}

fn replay_files(dir: &Path) -> Vec<PathBuf> {
  let mut files = fs::read_dir(dir)
    .ok()
    .into_iter()
    .flatten()
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| path.extension().and_then(|v| v.to_str()) == Some("ttrm"))
    .collect::<Vec<_>>();
  files.sort();
  files
}

fn parse_replay_date(doc: &Value) -> Option<DateTime<Utc>> {
  doc
    .pointer("/replay/ts")
    .and_then(Value::as_str)
    .or_else(|| doc.get("ts").and_then(Value::as_str))
    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
    .map(|d| d.with_timezone(&Utc))
}

fn find_rounds(doc: &Value, uid: &str) -> Vec<FoundRound> {
  let rounds = doc
    .pointer("/replay/replay/rounds")
    .and_then(Value::as_array)
    .cloned()
    .unwrap_or_default();

  let mut out = Vec::new();

  for round in rounds {
    let Some(players) = round.as_array() else {
      continue;
    };

    let Some(player) = players
      .iter()
      .find(|p| p.get("id").and_then(Value::as_str) == Some(uid))
      .cloned()
    else {
      continue;
    };

    let self_gameid = player
      .pointer("/replay/options/gameid")
      .and_then(as_i32_like);
    let opponents = players
      .iter()
      .filter_map(|item| item.pointer("/replay/options/gameid").and_then(as_i32_like))
      .filter(|id| Some(*id) != self_gameid)
      .collect::<Vec<_>>();
    let frames = player
      .pointer("/replay/events")
      .and_then(Value::as_array)
      .cloned()
      .unwrap_or_default();

    out.push(FoundRound {
      player,
      opponents,
      frames,
    });
  }

  out
}

fn convert_round(
  player: &Value,
  opponents: &[i32],
  date: Option<DateTime<Utc>>,
) -> EngineInitParams {
  let options = player.pointer("/replay/options").unwrap_or(&Value::Null);
  let handling = options.get("handling").unwrap_or(&Value::Null);
  let seed = get_i64(options, "seed", 0);
  let b2b_charging = get_bool(options, "b2bcharging", false);

  EngineInitParams {
    board: BoardInitParams {
      width: get_usize(options, "boardheight", 10),
      height: get_usize(options, "boardwidth", 20),
      buffer: 20,
    },
    kick_table: get_string(options, "kickset", "SRS+"),
    options: GameOptions {
      combo_table: get_string(options, "combotable", "multiplier"),
      garbage_blocking: get_string(options, "garbageblocking", "combo blocking"),
      clutch: get_bool(options, "clutch", true),
      garbage_target_bonus: get_string(options, "garbagetargetbonus", "none"),
      spin_bonuses: get_string(options, "spinbonuses", "all-mini+"),
      stock: 0,
    },
    queue: QueueInitParams {
      min_length: 10,
      seed,
      kind: parse_bag_type(get_optional_string(options, "bagtype").as_deref()),
    },
    garbage: GarbageQueueInitParams {
      bombs: get_bool(options, "usebombs", false),
      cap: GarbageCapParams {
        absolute: get_i32(options, "garbageabsolutecap", 0),
        increase: get_f64(options, "garbagecapincrease", 0.0),
        max: get_f64(options, "garbagecapmax", 40.0),
        value: get_f64(options, "garbagecap", 8.0),
        margin_time: get_i32(options, "garbagecapmargin", 0),
      },
      board_width: get_usize(options, "boardwidth", 10),
      garbage: GarbageSpeedParams {
        speed: get_i32(options, "garbagespeed", 20),
        hole_size: get_usize(options, "garbageholesize", 1),
      },
      messiness: MessinessParams {
        change: get_f64(options, "messiness_change", 1.0),
        nosame: get_bool(options, "messiness_nosame", false),
        timeout: get_i32(options, "messiness_timeout", 0),
        within: get_f64(options, "messiness_inner", 0.0),
        center: get_bool(options, "messiness_center", false),
      },
      multiplier: MultiplierParams {
        value: get_f64(options, "garbagemultiplier", 1.0),
        increase: get_f64(options, "garbageincrease", 0.008),
        margin_time: get_i32(options, "garbagemargin", 10800),
      },
      special_bonus: get_bool(options, "garbagespecialbonus", false),
      opener_phase: get_i32(options, "openerphase", 0),
      seed,
      rounding: parse_rounding_mode(get_optional_string(options, "roundmode").as_deref()),
    },
    gravity: IncreasableValue {
      value: get_f64(options, "g", 0.02),
      increase: get_f64(options, "gincrease", 0.0),
      margin_time: get_i32(options, "gmargin", 0),
    },
    handling: HandlingOptions {
      arr: get_f64(handling, "arr", 0.0),
      das: get_f64(handling, "das", 6.0),
      dcd: get_f64(handling, "dcd", 0.0),
      sdf: get_f64(handling, "sdf", 41.0),
      safelock: get_bool(handling, "safelock", false),
      cancel: get_bool(handling, "cancel", false),
      may20g: get_bool(handling, "may20g", true),
      irs: get_string(handling, "irs", "tap"),
      ihs: get_string(handling, "ihs", "tap"),
    },
    b2b: B2bOptions {
      chaining: !b2b_charging,
      charging: if b2b_charging {
        Some(B2bCharging {
          at: 4,
          base: get_i32(options, "b2bcharge_base", 3),
        })
      } else {
        None
      },
    },
    pc: Some(PcOptions {
      b2b: get_i32(options, "allclear_b2b", 0),
      garbage: get_f64(options, "allclear_garbage", 0.0),
    }),
    misc: MiscOptions {
      allowed: AllowedOptions {
        hard_drop: get_bool(options, "allow_harddrop", true),
        spin180: get_bool(options, "allow180", true),
        hold: get_bool(options, "display_hold", true),
        retry: get_bool(options, "can_retry", false),
        undo: get_bool(options, "can_undo", false),
      },
      infinite_hold: get_bool(options, "infinite_hold", false),
      movement: MovementOptions {
        infinite: false,
        lock_resets: get_i32(options, "lockresets", 15),
        lock_time: get_f64(options, "locktime", 30.0),
        may_20g: get_bool(options, "gravitymay20g", true),
      },
      username: get_optional_string(options, "username"),
      stride: get_bool(options, "stride", false),
      date,
    },
    multiplayer: Some(MultiplayerOptions {
      opponents: opponents.to_vec(),
      passthrough: get_string(options, "passthrough", "zero"),
    }),
  }
}

fn run_through(round: &ReplayRound) -> bool {
  let frames = split_frames(&round.frames);
  let mut engine = Engine::new(round.config.clone());

  while (engine.frame as usize) < frames.len() {
    let frame_index = engine.frame as usize;
    engine.tick(&frames[frame_index]);

    let topped_out = !legal(&engine.falling.absolute_blocks(), &engine.board.state);

    if topped_out && (engine.frame as usize) < frames.len().saturating_sub(10) {
      return false;
    }
  }

  true
}

fn split_frames(raw: &[Value]) -> Vec<Vec<ReplayFrame>> {
  assert!(!raw.is_empty(), "Replay is empty");

  let total_frames = raw
    .last()
    .and_then(|v| v.get("frame"))
    .and_then(as_usize_like)
    .unwrap_or(0)
    .saturating_add(1);

  let mut frames = Vec::with_capacity(total_frames + 1);
  let mut running_index = 0usize;

  for frame in 0..=total_frames {
    let mut bucket = Vec::new();
    while running_index < raw.len()
      && raw[running_index]
        .get("frame")
        .and_then(as_usize_like)
        .unwrap_or(usize::MAX)
        == frame
    {
      bucket.extend(parse_replay_frames(&raw[running_index]));
      running_index += 1;
    }
    frames.push(bucket);
  }

  frames
}

fn parse_replay_frames(frame: &Value) -> Vec<ReplayFrame> {
  let mut result = Vec::new();
  let frame_type = frame.get("type").and_then(Value::as_str).unwrap_or("");
  let subframe = frame
    .pointer("/data/subframe")
    .and_then(as_f64_like)
    .unwrap_or(0.0);

  match frame_type {
    "keydown" => {
      let key = frame
        .pointer("/data/key")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
      let hoisted = frame
        .pointer("/data/hoisted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
      result.push(ReplayFrame::Keydown(KeyEvent {
        subframe,
        key,
        hoisted,
      }));
    }
    "keyup" => {
      let key = frame
        .pointer("/data/key")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
      result.push(ReplayFrame::Keyup(KeyEvent {
        subframe,
        key,
        hoisted: false,
      }));
    }
    "ige" => {
      if let Some(ige_data) = parse_ige_data(frame, subframe) {
        result.push(ReplayFrame::Ige(ige_data));
      }
    }
    _ => {}
  }

  result
}

fn parse_ige_data(frame: &Value, subframe: f64) -> Option<IgeFrame> {
  let ige_type = frame.pointer("/data/type").and_then(Value::as_str)?;
  let payload = frame.pointer("/data/data").unwrap_or(&Value::Null);

  let data = match ige_type {
    "target" => {
      let targets = payload
        .get("targets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| as_i32_like(&value))
        .collect::<Vec<_>>();
      IgeData::Target { targets }
    }
    "interaction" => {
      if payload.get("type").and_then(Value::as_str) != Some("garbage") {
        return None;
      }
      IgeData::GarbageInteraction {
        gameid: get_i32(payload, "gameid", 0),
        ackiid: get_i32(payload, "ackiid", 0),
        iid: get_i32(payload, "iid", 0),
        amt: get_i32(payload, "amt", 0),
        size: get_usize(payload, "size", 0),
      }
    }
    "interaction_confirm" => {
      if payload.get("type").and_then(Value::as_str) != Some("garbage") {
        return None;
      }
      IgeData::GarbageConfirm {
        gameid: get_i32(payload, "gameid", 0),
        iid: get_i32(payload, "iid", 0),
        frame: get_i32(frame, "frame", 0),
      }
    }
    _ => return None,
  };

  Some(IgeFrame { subframe, data })
}

fn parse_bag_type(value: Option<&str>) -> BagType {
  match value.unwrap_or("7-bag") {
    "7-bag" | "bag7" => BagType::Bag7,
    "14-bag" | "bag14" => BagType::Bag14,
    "classic" => BagType::Classic,
    "pairs" => BagType::Pairs,
    "total mayhem" => BagType::TotalMayhem,
    "7+1" => BagType::Bag7Plus1,
    "7+2" => BagType::Bag7Plus2,
    "7+X" | "7+x" => BagType::Bag7PlusX,
    _ => BagType::Bag7,
  }
}

fn parse_rounding_mode(value: Option<&str>) -> RoundingMode {
  match value.unwrap_or("down").to_ascii_lowercase().as_str() {
    "rng" => RoundingMode::Rng,
    _ => RoundingMode::Down,
  }
}

fn get_optional_string(obj: &Value, key: &str) -> Option<String> {
  obj
    .get(key)
    .and_then(Value::as_str)
    .map(ToString::to_string)
}

fn get_string(obj: &Value, key: &str, default: &str) -> String {
  get_optional_string(obj, key).unwrap_or_else(|| default.to_string())
}

fn get_bool(obj: &Value, key: &str, default: bool) -> bool {
  obj.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn get_f64(obj: &Value, key: &str, default: f64) -> f64 {
  obj.get(key).and_then(as_f64_like).unwrap_or(default)
}

fn get_i64(obj: &Value, key: &str, default: i64) -> i64 {
  obj.get(key).and_then(as_i64_like).unwrap_or(default)
}

fn get_i32(obj: &Value, key: &str, default: i32) -> i32 {
  obj.get(key).and_then(as_i32_like).unwrap_or(default)
}

fn get_usize(obj: &Value, key: &str, default: usize) -> usize {
  obj.get(key).and_then(as_usize_like).unwrap_or(default)
}

fn as_f64_like(value: &Value) -> Option<f64> {
  value
    .as_f64()
    .or_else(|| value.as_i64().map(|v| v as f64))
    .or_else(|| value.as_u64().map(|v| v as f64))
}

fn as_i64_like(value: &Value) -> Option<i64> {
  value
    .as_i64()
    .or_else(|| value.as_u64().and_then(|v| i64::try_from(v).ok()))
    .or_else(|| value.as_f64().map(|v| v as i64))
}

fn as_i32_like(value: &Value) -> Option<i32> {
  as_i64_like(value).and_then(|v| i32::try_from(v).ok())
}

fn as_usize_like(value: &Value) -> Option<usize> {
  value
    .as_u64()
    .and_then(|v| usize::try_from(v).ok())
    .or_else(|| value.as_i64().and_then(|v| usize::try_from(v).ok()))
    .or_else(|| {
      value.as_f64().and_then(|v| {
        if v >= 0.0 {
          usize::try_from(v as u64).ok()
        } else {
          None
        }
      })
    })
}
