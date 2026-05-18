use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use msgpackr::{UnpackOptions, Unpacker, Value as MValue};
use triangle::engine::{
  AllowedOptions, B2bCharging, B2bOptions, Engine, EngineInitParams, GameOptions, IncreasableValue,
  MiscOptions, MovementOptions, MultiplayerOptions, PcOptions,
  board::BoardInitParams,
  garbage::{
    GarbageCapParams, GarbageQueueInitParams, GarbageSpeedParams, MessinessParams,
    MultiplierParams, RoundingMode,
  },
  queue::{QueueInitParams, bag::BagType},
  utils::KickTable,
};
use triangle::types::game::Passthrough;
use triangle::types::game::{
  Buffering, ComboTable, GarbageBlocking, GarbageTargetBonus, Handling, Key, SpinBonuses, ige,
  replay::{Frame, FrameData, Keypress},
};
use triangle::utils::Logger;

struct ReplayRound {
  id: String,
  index: usize,
  config: EngineInitParams,
  frames: Vec<MValue>,
}

fn make_unpacker() -> Unpacker {
  let mut unpacker = Unpacker::with_options(UnpackOptions {
    int64_as_type: Some("number".to_string()),
    ..Default::default()
  });
  unpacker.add_extension(1, |data| {
    let inner = if data.is_empty() {
      MValue::Nil
    } else {
      msgpackr::unpack(data)?
    };
    let mut pairs = vec![(MValue::Str("success".to_string()), MValue::Bool(true))];
    if let MValue::Map(extra) = inner {
      pairs.extend(extra);
    }
    Ok(MValue::Map(pairs))
  });
  unpacker.add_extension(2, |data| {
    let inner = if data.is_empty() {
      MValue::Nil
    } else {
      msgpackr::unpack(data)?
    };
    let mut pairs = vec![(MValue::Str("success".to_string()), MValue::Bool(false))];
    if !matches!(inner, MValue::Nil) {
      pairs.push((MValue::Str("error".to_string()), inner));
    }
    Ok(MValue::Map(pairs))
  });
  unpacker
}

fn process_file(file: PathBuf) -> Result<(), String> {
  let unpacker = make_unpacker();

  let bytes =
    fs::read(&file).map_err(|e| format!("Failed to read replay file {}: {e}", file.display()))?;
  let replay_doc = unpacker
    .unpack(&bytes)
    .map_err(|e| format!("Failed to parse replay file {}: {e}", file.display()))?;

  let replay_id = file
    .file_stem()
    .and_then(|v| v.to_str())
    .unwrap_or("<unknown>")
    .to_string();
  let user_id = mvalue_pointer(&replay_doc, "/user/id")
    .and_then(as_str_like)
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
    if !run_through(&round) {
      return Err(format!(
        "Failure at: https://tetr.io/#R:{}@{}",
        round.id,
        round.index + 1
      ));
    }
  }

  Ok(())
}

#[tokio::test]
async fn replay_test() {
  let replay_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/replays");
  let files = replay_files(&replay_dir);

  assert!(!files.is_empty(), "No replays found.");

  let len = files.len();
  let mut logger = Logger::new("triangle-rs");
  logger.progress("Running replays...", 0.0);

  let mut join_set = tokio::task::JoinSet::new();
  for file in files {
    join_set.spawn_blocking(move || process_file(file));
  }

  let mut completed = 0usize;
  let mut failures: Vec<String> = Vec::new();

  while let Some(result) = join_set.join_next().await {
    completed += 1;
    logger.progress("Running replays...", completed as f64 / len as f64);
    match result {
      Ok(Ok(())) => {}
      Ok(Err(e)) => failures.push(e),
      Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
      Err(e) => panic!("Task cancelled: {e}"),
    }
  }

  println!();

  if !failures.is_empty() {
    panic!("Failures:\n{}", failures.join("\n"));
  }
}

struct FoundRound {
  player: MValue,
  opponents: Vec<i32>,
  frames: Vec<MValue>,
}

fn replay_files(dir: &Path) -> Vec<PathBuf> {
  let mut files = fs::read_dir(dir)
    .ok()
    .into_iter()
    .flatten()
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| path.extension().and_then(|v| v.to_str()) == Some("ttrmx"))
    .collect::<Vec<_>>();
  files.sort();
  files
}

fn parse_replay_date(doc: &MValue) -> Option<DateTime<Utc>> {
  mvalue_pointer(doc, "/replay/ts")
    .and_then(as_str_like)
    .or_else(|| mvalue_get(doc, "ts").and_then(as_str_like))
    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
    .map(|d| d.with_timezone(&Utc))
}

fn find_rounds(doc: &MValue, uid: &str) -> Vec<FoundRound> {
  let rounds = mvalue_pointer(doc, "/replay/replay/rounds")
    .and_then(as_array_like)
    .cloned()
    .unwrap_or_default();

  let mut out = Vec::new();

  for round in rounds {
    let Some(players) = as_array_like(&round) else {
      continue;
    };

    let Some(player) = players
      .iter()
      .find(|p| mvalue_get(p, "id").and_then(as_str_like) == Some(uid))
      .cloned()
    else {
      continue;
    };

    let active = mvalue_get(&player, "active")
      .and_then(as_bool_like)
      .unwrap_or(false);
    if !active {
      continue;
    }

    let self_gameid = mvalue_pointer(&player, "/replay/options/gameid").and_then(as_i32_like);
    let opponents = players
      .iter()
      .filter(|item| {
        mvalue_get(item, "active")
          .and_then(as_bool_like)
          .unwrap_or(false)
      })
      .filter_map(|item| mvalue_pointer(item, "/replay/options/gameid").and_then(as_i32_like))
      .filter(|id| Some(*id) != self_gameid)
      .collect::<Vec<_>>();

    if opponents.is_empty() {
      continue;
    }

    let frames = mvalue_pointer(&player, "/replay/events")
      .and_then(as_array_like)
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
  player: &MValue,
  opponents: &[i32],
  date: Option<DateTime<Utc>>,
) -> EngineInitParams {
  let options = mvalue_pointer(player, "/replay/options").unwrap_or(&MValue::Nil);
  let handling = mvalue_get(options, "handling").unwrap_or(&MValue::Nil);
  let seed = get_i64(options, "seed", 0);
  let b2b_charging = get_bool(options, "b2bcharging", false);

  EngineInitParams {
    board: BoardInitParams {
      width: get_usize(options, "boardheight", 10),
      height: get_usize(options, "boardwidth", 20),
      buffer: 20,
    },
    kick_table: parse_kick_table(
      get_optional_string(options, "kickset")
        .as_deref()
        .unwrap_or("SRS+"),
    ),
    options: GameOptions {
      combo_table: parse_serde_enum(
        get_optional_string(options, "combotable")
          .as_deref()
          .unwrap_or("multiplier"),
        ComboTable::Multiplier,
      ),
      garbage_blocking: parse_serde_enum(
        get_optional_string(options, "garbageblocking")
          .as_deref()
          .unwrap_or("combo blocking"),
        GarbageBlocking::ComboBlocking,
      ),
      clutch: get_bool(options, "clutch", true),
      garbage_target_bonus: parse_serde_enum(
        get_optional_string(options, "garbagetargetbonus")
          .as_deref()
          .unwrap_or("none"),
        GarbageTargetBonus::None,
      ),
      spin_bonuses: parse_serde_enum(
        get_optional_string(options, "spinbonuses")
          .as_deref()
          .unwrap_or("all-mini+"),
        SpinBonuses::AllMiniPlus,
      ),
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
        absolute: get_i32(options, "garbageabsolutecap", 0) as u32,
        increase: get_f64(options, "garbagecapincrease", 0.0),
        max: get_f64(options, "garbagecapmax", 40.0),
        value: get_f64(options, "garbagecap", 8.0),
        margin_time: get_i32(options, "garbagecapmargin", 0) as u64,
      },
      board_width: get_usize(options, "boardwidth", 10),
      garbage: GarbageSpeedParams {
        speed: get_i32(options, "garbagespeed", 20) as u64,
        hole_size: get_usize(options, "garbageholesize", 1) as u64,
      },
      messiness: MessinessParams {
        change: get_f64(options, "messiness_change", 1.0),
        nosame: get_bool(options, "messiness_nosame", false),
        timeout: get_i32(options, "messiness_timeout", 0) as u64,
        within: get_f64(options, "messiness_inner", 0.0),
        center: get_bool(options, "messiness_center", false),
      },
      multiplier: MultiplierParams {
        value: get_f64(options, "garbagemultiplier", 1.0),
        increase: get_f64(options, "garbageincrease", 0.008),
        margin_time: get_i32(options, "garbagemargin", 10800) as u64,
      },
      special_bonus: get_bool(options, "garbagespecialbonus", false),
      opener_phase: get_i32(options, "openerphase", 0) as u32,
      seed,
      rounding: parse_rounding_mode(get_optional_string(options, "roundmode").as_deref()),
    },
    gravity: IncreasableValue {
      value: get_f64(options, "g", 0.02),
      increase: get_f64(options, "gincrease", 0.0),
      margin_time: get_i32(options, "gmargin", 0) as u64,
    },
    handling: Handling {
      arr: get_f64(handling, "arr", 0.0),
      das: get_f64(handling, "das", 6.0),
      dcd: get_f64(handling, "dcd", 0.0),
      sdf: get_f64(handling, "sdf", 41.0),
      safelock: get_bool(handling, "safelock", false),
      cancel: get_bool(handling, "cancel", false),
      may20g: get_bool(handling, "may20g", true),
      irs: parse_buffering(
        get_optional_string(handling, "irs")
          .as_deref()
          .unwrap_or("tap"),
      ),
      ihs: parse_buffering(
        get_optional_string(handling, "ihs")
          .as_deref()
          .unwrap_or("tap"),
      ),
    },
    b2b: B2bOptions {
      chaining: !b2b_charging,
      charging: if b2b_charging {
        Some(B2bCharging {
          at: 4,
          base: get_i32(options, "b2bcharge_base", 3) as u64,
        })
      } else {
        None
      },
    },
    pc: Some(PcOptions {
      b2b: get_i32(options, "allclear_b2b", 0) as u64,
      garbage: get_i32(options, "allclear_garbage", 0) as u32,
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
        lock_resets: get_i32(options, "lockresets", 15) as u64,
        lock_time: get_f64(options, "locktime", 30.0),
        may_20g: get_bool(options, "gravitymay20g", true),
      },
      username: get_optional_string(options, "username"),
      stride: get_bool(options, "stride", false),
      date,
    },
    multiplayer: Some(MultiplayerOptions {
      opponents: opponents.iter().map(|&x| x as u64).collect(),
      passthrough: parse_serde_enum(
        get_optional_string(options, "passthrough")
          .as_deref()
          .unwrap_or("zero"),
        Passthrough::Zero,
      ),
    }),
  }
}

fn run_through(round: &ReplayRound) -> bool {
  let frames = split_frames(&round.frames);
  let mut engine = Engine::new(round.config.clone());

  while (engine.frame as usize) < frames.len() {
    let frame_index = engine.frame as usize;
    engine.tick(&frames[frame_index]);

    // if engine.frame.is_multiple_of(30) {
    //   println!(
    //     "Frame {}",
    // 		engine.frame
    //   );

    // 	engine.print();
    // }

    if engine.topped_out() && (engine.frame as usize) < frames.len().saturating_sub(10) {
      return false;
    }
  }

  true
}

fn split_frames(raw: &[MValue]) -> Vec<Vec<Frame>> {
  assert!(!raw.is_empty(), "Replay is empty");

  let total_frames = raw
    .last()
    .and_then(|v| mvalue_get(v, "frame"))
    .and_then(as_usize_like)
    .unwrap_or(0)
    .saturating_add(1);

  let mut frames = Vec::with_capacity(total_frames + 1);
  let mut running_index = 0usize;

  for frame in 0..=total_frames {
    let mut bucket = Vec::new();
    while running_index < raw.len()
      && mvalue_get(&raw[running_index], "frame")
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

fn parse_replay_frames(raw_frame: &MValue) -> Vec<Frame> {
  let frame_num = mvalue_get(raw_frame, "frame")
    .and_then(as_usize_like)
    .unwrap_or(0) as u64;
  let frame_type = mvalue_get(raw_frame, "type")
    .and_then(as_str_like)
    .unwrap_or("");
  let subframe = mvalue_pointer(raw_frame, "/data/subframe")
    .and_then(as_f64_like)
    .unwrap_or(0.0);

  match frame_type {
    "keydown" => {
      let key_str = mvalue_pointer(raw_frame, "/data/key")
        .and_then(as_str_like)
        .unwrap_or("");
      let hoisted = mvalue_pointer(raw_frame, "/data/hoisted")
        .and_then(as_bool_like)
        .unwrap_or(false);
      if let Some(key) = parse_key(key_str) {
        vec![Frame {
          frame: frame_num,
          data: FrameData::KeyDown(Keypress {
            key,
            subframe,
            hoisted,
          }),
        }]
      } else {
        vec![]
      }
    }
    "keyup" => {
      let key_str = mvalue_pointer(raw_frame, "/data/key")
        .and_then(as_str_like)
        .unwrap_or("");
      if let Some(key) = parse_key(key_str) {
        vec![Frame {
          frame: frame_num,
          data: FrameData::KeyUp(Keypress {
            key,
            subframe,
            hoisted: false,
          }),
        }]
      } else {
        vec![]
      }
    }
    "ige" => {
      if let Some(data) = mvalue_get(raw_frame, "data") {
        if let Ok(ige_val) = msgpackr::serde::from_value::<ige::IGE>(data.clone()) {
          vec![Frame {
            frame: frame_num,
            data: FrameData::IGE(ige_val),
          }]
        } else {
          vec![]
        }
      } else {
        vec![]
      }
    }
    _ => vec![],
  }
}

fn parse_key(s: &str) -> Option<Key> {
  match s {
    "moveLeft" => Some(Key::MoveLeft),
    "moveRight" => Some(Key::MoveRight),
    "rotateCW" => Some(Key::RotateCW),
    "rotateCCW" => Some(Key::RotateCCW),
    "rotate180" => Some(Key::Rotate180),
    "softDrop" => Some(Key::SoftDrop),
    "hardDrop" => Some(Key::HardDrop),
    "hold" => Some(Key::Hold),
    "undo" => Some(Key::Undo),
    "redo" => Some(Key::Redo),
    "retry" => Some(Key::Retry),
    _ => None,
  }
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

fn get_optional_string(obj: &MValue, key: &str) -> Option<String> {
  mvalue_get(obj, key)
    .and_then(as_str_like)
    .map(ToString::to_string)
}

fn parse_kick_table(s: &str) -> KickTable {
  match s {
    "SRS+" => KickTable::SRSPlus,
    "SRS-X" | "SRSX" => KickTable::SRSX,
    "TETRA-X" | "TetraX" => KickTable::TetraX,
    "SRS" => KickTable::SRS,
    "NRS" => KickTable::NRS,
    "ARS" => KickTable::ARS,
    "ASC" => KickTable::ASC,
    "none" | "None" => KickTable::None,
    _ => KickTable::SRSPlus,
  }
}

fn parse_buffering(s: &str) -> Buffering {
  match s {
    "off" => Buffering::Off,
    "hold" => Buffering::Hold,
    _ => Buffering::Tap,
  }
}

fn parse_serde_enum<T: serde::de::DeserializeOwned>(s: &str, default: T) -> T {
  msgpackr::serde::from_value(MValue::Str(s.to_string())).unwrap_or(default)
}

fn get_bool(obj: &MValue, key: &str, default: bool) -> bool {
  mvalue_get(obj, key)
    .and_then(as_bool_like)
    .unwrap_or(default)
}

fn get_f64(obj: &MValue, key: &str, default: f64) -> f64 {
  mvalue_get(obj, key)
    .and_then(as_f64_like)
    .unwrap_or(default)
}

fn get_i64(obj: &MValue, key: &str, default: i64) -> i64 {
  mvalue_get(obj, key)
    .and_then(as_i64_like)
    .unwrap_or(default)
}

fn get_i32(obj: &MValue, key: &str, default: i32) -> i32 {
  mvalue_get(obj, key)
    .and_then(as_i32_like)
    .unwrap_or(default)
}

fn get_usize(obj: &MValue, key: &str, default: usize) -> usize {
  mvalue_get(obj, key)
    .and_then(as_usize_like)
    .unwrap_or(default)
}

fn mvalue_get<'a>(val: &'a MValue, key: &str) -> Option<&'a MValue> {
  match val {
    MValue::Map(pairs) => pairs.iter().find_map(|(k, v)| {
      if matches!(k, MValue::Str(s) if s == key) {
        Some(v)
      } else {
        None
      }
    }),
    _ => None,
  }
}

fn mvalue_pointer<'a>(val: &'a MValue, path: &str) -> Option<&'a MValue> {
  let mut current = val;
  for part in path.split('/').filter(|s| !s.is_empty()) {
    current = mvalue_get(current, part)?;
  }
  Some(current)
}

fn as_bool_like(value: &MValue) -> Option<bool> {
  match value {
    MValue::Bool(b) => Some(*b),
    _ => None,
  }
}

fn as_str_like(value: &MValue) -> Option<&str> {
  match value {
    MValue::Str(s) => Some(s.as_str()),
    _ => None,
  }
}

fn as_array_like(value: &MValue) -> Option<&Vec<MValue>> {
  match value {
    MValue::Array(arr) => Some(arr),
    _ => None,
  }
}

fn as_f64_like(value: &MValue) -> Option<f64> {
  match value {
    MValue::F64(v) => Some(*v),
    MValue::F32(v) => Some(*v as f64),
    MValue::Int(v) => Some(*v as f64),
    MValue::UInt(v) => Some(*v as f64),
    _ => None,
  }
}

fn as_i64_like(value: &MValue) -> Option<i64> {
  match value {
    MValue::Int(v) => Some(*v),
    MValue::UInt(v) => i64::try_from(*v).ok(),
    MValue::F64(v) => Some(*v as i64),
    MValue::F32(v) => Some(*v as i64),
    _ => None,
  }
}

fn as_i32_like(value: &MValue) -> Option<i32> {
  as_i64_like(value).and_then(|v| i32::try_from(v).ok())
}

fn as_usize_like(value: &MValue) -> Option<usize> {
  match value {
    MValue::UInt(v) => usize::try_from(*v).ok(),
    MValue::Int(v) => usize::try_from(*v).ok(),
    MValue::F64(v) => {
      if *v >= 0.0 {
        usize::try_from(*v as u64).ok()
      } else {
        None
      }
    }
    MValue::F32(v) => {
      if *v >= 0.0 {
        usize::try_from(*v as u64).ok()
      } else {
        None
      }
    }
    _ => None,
  }
}
