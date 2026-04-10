#[derive(Debug, Clone)]
pub struct IncreaseTracker {
  base: f64,
  increase: f64,
  margin: u32,
  frame: u32,
  value: f64,
}

impl IncreaseTracker {
  pub fn new(base: f64, increase: f64, margin: u32) -> Self {
    Self {
      base,
      increase,
      margin,
      frame: 0,
      value: base,
    }
  }

  pub fn tick(&mut self) {
    self.frame += 1;
    if self.frame > self.margin && self.increase > 0.0 {
      self.value += self.increase / 60.0;
    }
  }

  pub fn get(&self) -> f64 {
    self.value
  }

  pub fn set(&mut self, value: f64) {
    self.value = value;
  }

  pub fn reset(&mut self) {
    self.frame = 0;
    self.value = self.base;
  }

  pub fn base(&self) -> f64 {
    self.base
  }

  pub fn increase(&self) -> f64 {
    self.increase
  }

  pub fn margin(&self) -> u32 {
    self.margin
  }
}
