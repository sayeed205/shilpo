//! Typed Material 3 foundation values and component token sets.
//!
//! This module is data-only. It does not render components, resolve colors,
//! play motion, or impose shell policy.

pub mod color_role;
pub mod components;
pub mod foundation;
pub mod motion;
pub mod palette;
pub mod shape;
pub mod typography;
pub mod units;

#[cfg(test)]
#[path = "tests/reference.rs"]
mod reference;
