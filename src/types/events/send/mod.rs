use crate::macros::event;

pub use super::recv::client;

pub mod config;
pub mod room;
pub mod social;

event!(die => Die);
