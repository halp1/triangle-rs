use crate::engine::queue::types::Mino;

const SINGLE: f64 = 0.0;
const DOUBLE: f64 = 1.0;
const TRIPLE: f64 = 2.0;
const QUAD: f64 = 4.0;
const PENTA: f64 = 5.0;
const TSPIN_MINI: f64 = 0.0;
const TSPIN: f64 = 0.0;
const TSPIN_MINI_SINGLE: f64 = 0.0;
const TSPIN_SINGLE: f64 = 2.0;
const TSPIN_MINI_DOUBLE: f64 = 1.0;
const TSPIN_MINI_TRIPLE: f64 = 2.0;
const TSPIN_DOUBLE: f64 = 4.0;
const TSPIN_TRIPLE: f64 = 6.0;
const TSPIN_QUAD: f64 = 10.0;
const TSPIN_PENTA: f64 = 12.0;
const BACK_TO_BACK_BONUS: f64 = 1.0;
const BACK_TO_BACK_BONUS_LOG: f64 = 0.8;
const COMBO_MINIFIER: f64 = 1.0;
const COMBO_MINIFIER_LOG: f64 = 1.25;
const COMBO_BONUS: f64 = 0.25;
pub const ALL_CLEAR: f64 = 10.0;

static COMBO_TABLE_NONE: &[i32] = &[0];
static COMBO_TABLE_CLASSIC: &[i32] = &[0, 1, 1, 2, 2, 3, 3, 4, 4, 4, 5];
static COMBO_TABLE_MODERN: &[i32] = &[0, 1, 1, 2, 2, 2, 3, 3, 3, 3, 3, 3, 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinType {
  None,
  Mini,
  Normal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComboTable {
  None,
  ClassicGuideline,
  ModernGuideline,
  Multiplier,
}

#[derive(Debug, Clone)]
pub struct GarbageCalcConfig {
  pub spin_bonuses: String,
  pub combo_table: ComboTable,
  pub garbage_target_bonus: String,
  pub b2b_chaining: bool,
  pub b2b_charging: bool,
}

#[derive(Debug, Clone)]
pub struct GarbageCalcInput {
  pub lines: i32,
  pub spin: SpinType,
  pub piece: Mino,
  pub b2b: i32,
  pub combo: i32,
  pub enemies: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct GarbageCalcOutput {
  pub garbage: f64,
  pub bonus: f64,
}

pub fn garbage_calc_v2(input: &GarbageCalcInput, config: &GarbageCalcConfig) -> GarbageCalcOutput {
  let GarbageCalcInput {
    lines,
    spin,
    piece,
    combo,
    b2b,
    enemies,
  } = *input;

  let spin_opt = match spin {
    SpinType::None => None,
    SpinType::Mini => Some(SpinType::Mini),
    SpinType::Normal => Some(SpinType::Normal),
  };

  let mut garbage: f64 = match lines {
    0 => match spin_opt {
      Some(SpinType::Mini) => TSPIN_MINI,
      Some(SpinType::Normal) => TSPIN,
      _ => 0.0,
    },
    1 => match spin_opt {
      Some(SpinType::Mini) => TSPIN_MINI_SINGLE,
      Some(SpinType::Normal) => TSPIN_SINGLE,
      _ => SINGLE,
    },
    2 => match spin_opt {
      Some(SpinType::Mini) => TSPIN_MINI_DOUBLE,
      Some(SpinType::Normal) => TSPIN_DOUBLE,
      _ => DOUBLE,
    },
    3 => match spin_opt {
      Some(SpinType::Mini) => TSPIN_MINI_TRIPLE,
      Some(SpinType::Normal) => TSPIN_TRIPLE,
      _ => TRIPLE,
    },
    4 => {
      if spin_opt.is_some() {
        TSPIN_QUAD
      } else {
        QUAD
      }
    }
    5 => {
      if spin_opt.is_some() {
        TSPIN_PENTA
      } else {
        PENTA
      }
    }
    n => {
      let t = (n - 5) as f64;
      if spin_opt.is_some() {
        TSPIN_PENTA + 2.0 * t
      } else {
        PENTA + t
      }
    }
  };

  if spin_opt.is_some() && config.spin_bonuses == "handheld" && piece != Mino::T {
    garbage /= 2.0;
  }

  if lines > 0 && b2b > 0 {
    if config.b2b_chaining {
      let b2b_f = b2b as f64;
      let log_val = (1.0 + b2b_f * BACK_TO_BACK_BONUS_LOG).ln_1p();
      let extra = if b2b == 1 {
        0.0
      } else {
        (1.0 + (b2b_f * BACK_TO_BACK_BONUS_LOG).ln_1p() % 1.0) / 3.0
      };
      garbage += BACK_TO_BACK_BONUS * (((1.0 + log_val).floor()) + extra);
    } else {
      garbage += BACK_TO_BACK_BONUS;
    }
  }

  if combo > 0 {
    match config.combo_table {
      ComboTable::Multiplier => {
        garbage *= 1.0 + COMBO_BONUS * combo as f64;
        if combo > 1 {
          let min_val = (COMBO_MINIFIER * combo as f64 * COMBO_MINIFIER_LOG + 1.0).ln();
          garbage = garbage.max(min_val);
        }
      }
      ref table => {
        let table_data: &[i32] = match table {
          ComboTable::None => COMBO_TABLE_NONE,
          ComboTable::ClassicGuideline => COMBO_TABLE_CLASSIC,
          ComboTable::ModernGuideline => COMBO_TABLE_MODERN,
          _ => COMBO_TABLE_NONE,
        };
        let idx = ((combo - 1) as usize).min(table_data.len().saturating_sub(1));
        garbage += table_data[idx] as f64;
      }
    }
  }

  let mut garbage_bonus: f64 = 0.0;
  if lines > 0 && config.garbage_target_bonus != "none" {
    let target_bonus: f64 = match enemies {
      0 | 1 => 0.0,
      2 => 1.0,
      3 => 3.0,
      4 => 5.0,
      5 => 7.0,
      _ => 9.0,
    };

    if config.garbage_target_bonus == "normal" {
      garbage += target_bonus;
    } else {
      garbage_bonus = target_bonus;
    }
  }

  GarbageCalcOutput {
    garbage,
    bonus: garbage_bonus,
  }
}
