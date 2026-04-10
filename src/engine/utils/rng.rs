const MODULUS: i64 = 2_147_483_647;
const MULTIPLIER: i64 = 16_807;
const MAX_FLOAT: f64 = 2_147_483_646.0;

#[derive(Debug, Clone)]
pub struct Rng {
  seed: i64,
  index: u64,
}

impl Rng {
  pub fn new(seed: i64) -> Self {
    let mut value = seed.rem_euclid(MODULUS);
    if value <= 0 {
      value += MAX_FLOAT as i64;
    }
    Self {
      seed: value,
      index: 0,
    }
  }

  pub fn seed(&self) -> i64 {
    self.seed
  }

  pub fn set_seed(&mut self, seed: i64) {
    let mut value = seed.rem_euclid(MODULUS);
    if value <= 0 {
      value += MAX_FLOAT as i64;
    }
    self.seed = value;
  }

  pub fn next(&mut self) -> i64 {
    self.index += 1;
    self.seed = (self.seed * MULTIPLIER).rem_euclid(MODULUS);
    self.seed
  }

  pub fn next_float(&mut self) -> f64 {
    (self.next() as f64 - 1.0) / MAX_FLOAT
  }

  pub fn shuffle_array<T>(&mut self, arr: &mut Vec<T>) {
    let len = arr.len();
    for i in (1..len).rev() {
      let j = (self.next_float() * (i + 1) as f64).floor() as usize;
      arr.swap(i, j);
    }
  }

  pub fn update_from_index(&mut self, index: u64) {
    while self.index < index {
      self.next();
    }
  }
}
