use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mino {
  #[serde(rename = "i")]
  I,
  #[serde(rename = "j")]
  J,
  #[serde(rename = "l")]
  L,
  #[serde(rename = "o")]
  O,
  #[serde(rename = "s")]
  S,
  #[serde(rename = "t")]
  T,
  #[serde(rename = "z")]
  Z,
  #[serde(rename = "gb")]
  Garbage,
  #[serde(rename = "bomb")]
  Bomb,
}

impl Mino {
  pub fn is_standard(&self) -> bool {
    matches!(
      self,
      Mino::I | Mino::J | Mino::L | Mino::O | Mino::S | Mino::T | Mino::Z
    )
  }

  pub fn as_str(&self) -> &'static str {
    match self {
      Mino::I => "i",
      Mino::J => "j",
      Mino::L => "l",
      Mino::O => "o",
      Mino::S => "s",
      Mino::T => "t",
      Mino::Z => "z",
      Mino::Garbage => "gb",
      Mino::Bomb => "bomb",
    }
  }
}

impl std::fmt::Display for Mino {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.as_str())
  }
}
