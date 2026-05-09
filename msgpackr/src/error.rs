use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum Error {
  #[error("Unexpected end of MessagePack data")]
  UnexpectedEnd,

  #[error("Unknown extension type: {0}")]
  UnknownExtension(u8),

  #[error("Invalid data: {0}")]
  InvalidData(String),

  #[error("Range error: {0}")]
  RangeError(String),

  #[error("Object too large: {0}")]
  TooLarge(String),
}

impl Error {
  pub fn incomplete(&self) -> bool {
    matches!(self, Error::UnexpectedEnd)
  }

  pub fn invalid(msg: impl Into<String>) -> Self {
    Error::InvalidData(msg.into())
  }
}

impl serde::ser::Error for Error {
  fn custom<T: std::fmt::Display>(msg: T) -> Self {
    Error::InvalidData(msg.to_string())
  }
}

impl serde::de::Error for Error {
  fn custom<T: std::fmt::Display>(msg: T) -> Self {
    Error::InvalidData(msg.to_string())
  }
}

pub type Result<T> = std::result::Result<T, Error>;
