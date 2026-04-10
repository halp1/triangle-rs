use rand::Rng;

pub fn random_seed() -> i64 {
  rand::thread_rng().gen_range(0..2_147_483_646)
}
