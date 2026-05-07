use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{classes::game::player::Player, types::game::SpectatingStrategy};

use super::ribbon::{Ribbon, Hook};

pub mod player;
pub mod me;

pub const FRAMES_PER_SECOND: u64 = 60;

#[derive(Debug, Clone)]
pub struct GameState {
	pub strategy: SpectatingStrategy,
	pub spectating_loop_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>, // very cursed, maybe a better way?
	pub spectate_warning_counter: u64,
	pub players: Vec<Player>
}

#[derive(Debug)]
pub struct Game {
	ribbon: Ribbon,
	hook: Hook,
	
}