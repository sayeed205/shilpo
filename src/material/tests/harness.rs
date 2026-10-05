#![allow(dead_code)]

#[path = "../mod.rs"]
mod material;

#[cfg(test)]
pub mod approx {
    pub use crate::assert_relative_eq;
}

#[cfg(test)]
#[macro_export]
macro_rules! assert_relative_eq {
    ($left:expr, $right:expr, epsilon = $epsilon:expr $(,)?) => {{
        let (left, right, epsilon) = ($left, $right, $epsilon);
        assert!(
            (left - right).abs() <= epsilon,
            "{left:?} != {right:?} within {epsilon:?}"
        );
    }};
    ($left:expr, $right:expr $(,)?) => {{
        let (left, right) = ($left, $right);
        assert!((left - right).abs() <= 1e-12, "{left:?} != {right:?}");
    }};
}

fn main() {}
