pub trait Partial: Sized {
  type Target;

  fn merge(self, other: Self) -> Self;
  fn apply(self, target: &mut Self::Target);
}
