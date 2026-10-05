// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 JAC and Contributors
//
// This module contains the Rust adaptation of Google's Material Color Utilities
// (Copyright 2021 Google LLC), Apache-2.0 licensed.

mod core_palette;
mod tonal_palette;

pub use core_palette::{CorePalette, CorePaletteColors};
pub use tonal_palette::TonalPalette;
