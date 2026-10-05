// Standalone Material Color implementation. The implementation modules remain
// private; callers enter through Scheme and the small set of value types below.

mod contrast;
mod dislike;
mod dynamiccolor;
mod hct;
mod options;
mod palettes;
mod roles;
mod scheme;
mod temperature;
mod utils;

pub use dynamiccolor::{Platform, Variant};
pub use options::SchemeOptions;
pub use palettes::TonalPalette as Palette;
pub use roles::MaterialRole as Role;
pub use scheme::{Scheme, SchemeError};

#[cfg(test)]
#[path = "tests/reference_vectors.rs"]
mod reference_vectors;
