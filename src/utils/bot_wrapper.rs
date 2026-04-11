use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
  Engine,
  engine::{KeyEvent, ReplayFrame},
  error::{Result, TriangleError},
};

use super::adapters::{Adapter, AdapterKey};

#[derive(Debug, Clone, Copy)]
pub struct BotWrapperConfig {
  pub pps: f64,
}

pub struct BotWrapper<A, T = Value>
where
  A: Adapter<T>,
  T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
  pub config: BotWrapperConfig,
  pub adapter: A,
  pub next_frame: f64,
  pub needs_new_move: bool,
  _marker: std::marker::PhantomData<T>,
}

impl<A, T> BotWrapper<A, T>
where
  A: Adapter<T>,
  T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
  pub fn new(adapter: A, config: BotWrapperConfig) -> Self {
    Self {
      config,
      adapter,
      next_frame: 0.0,
      needs_new_move: false,
      _marker: std::marker::PhantomData,
    }
  }

  pub fn next_frame(engine: &Engine, target: f64) -> f64 {
    ((engine.stats.pieces as f64 + 1.0) / target) * 60.0
  }

  pub fn frames(engine: &Engine, keys: &[AdapterKey]) -> Vec<ReplayFrame> {
    let mut running = engine.frame as f64 + engine.subframe;

    keys
      .iter()
      .flat_map(|key| {
        let start_subframe = round1(running - running.floor());
        let press = ReplayFrame::Keydown(KeyEvent {
          subframe: start_subframe,
          key: key.as_input_key().to_string(),
          hoisted: false,
        });

        if matches!(key, AdapterKey::DasLeft | AdapterKey::DasRight) {
          running = round1(running + engine.handling.das);
        } else if matches!(key, AdapterKey::SoftDrop) {
          running = round1(running + 0.1);
        }

        let release = ReplayFrame::Keyup(KeyEvent {
          subframe: round1(running - running.floor()),
          key: key.as_input_key().to_string(),
          hoisted: false,
        });

        [press, release]
      })
      .collect()
  }

  pub async fn init(&mut self, engine: &Engine, data: Option<T>) -> Result<()> {
    if engine.handling.arr != 0.0 {
      return Err(TriangleError::Adapter(
        "BotWrapper requires 0 ARR handling.".to_string(),
      ));
    }
    if engine.handling.sdf != 41.0 {
      return Err(TriangleError::Adapter(
        "BotWrapper requires 41 SDF handling.".to_string(),
      ));
    }

    self.adapter.initialize().await?;
    self.adapter.config(engine, data).await?;
    self.next_frame = Self::next_frame(engine, self.config.pps);
    Ok(())
  }

  pub async fn tick(
    &mut self,
    engine: &Engine,
    has_garbage_event: bool,
    data: Option<T>,
  ) -> Result<Vec<ReplayFrame>> {
    if has_garbage_event {
      self.adapter.update(engine, data.clone()).await?;
    }

    if engine.frame as f64 >= self.next_frame {
      if self.needs_new_move {
        self.next_frame = Self::next_frame(engine, self.config.pps);
        self.needs_new_move = false;
      } else {
        let movement = self.adapter.play(engine, data).await?;
        self.needs_new_move = true;
        return Ok(Self::frames(engine, &movement.keys));
      }
    }

    Ok(Vec::new())
  }

  pub async fn stop(&mut self) -> Result<()> {
    self.adapter.stop().await
  }
}

fn round1(value: f64) -> f64 {
  (value * 10.0).round() / 10.0
}
