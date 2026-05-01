use crate::macros::event;

pub use super::recv::client;

pub mod social;
pub mod config;

event!(die => Die);