pub const USER_AGENT: &str = "Triangle.rs/4.2.5 (+https://triangle.haelp.dev)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Constants {
  pub user_agent: &'static str,
}

pub const CONSTANTS: Constants = Constants {
  user_agent: USER_AGENT,
};
