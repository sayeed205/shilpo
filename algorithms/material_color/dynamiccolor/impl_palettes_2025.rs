// <FILE>crates/mcu-dynamiccolor/src/impl_palettes_2025.rs</FILE> - <DESC>2025 spec palette generation delegate implementation</DESC>
// <VERS>VERSION: 1.0.0</VERS>
// <WCTX>OFPF refactor: Extract 2025 palette delegate from dynamic_scheme_palettes.rs</WCTX>
// <CLOG>Initial extraction of DynamicSchemePalettesDelegateImpl2025 to comply with OFPF LOC limits</CLOG>

//! 2025 Material Design specification palette generation delegate.
//!
//! This module contains the palette generation logic for the updated
//! Material Design 3 specification (2025), which introduces platform-aware
//! palette generation and additional adjustments based on blue hue and dark mode.

use crate::material_color::hct::Hct;
use crate::material_color::palettes::TonalPalette;

use super::dynamic_scheme_palettes::{
    get_piecewise_value, get_rotated_hue, DynamicSchemePalettesDelegate,
};
use super::impl_palettes_2021::DynamicSchemePalettesDelegateImpl2021;
use super::{Platform, Variant};

/// Palette generation delegate for the 2025 Material Design 3 specification.
///
/// The 2025 spec introduces platform-aware palette generation and additional
/// adjustments based on whether the color is blue and the dark/light mode.
pub struct DynamicSchemePalettesDelegateImpl2025;

impl DynamicSchemePalettesDelegateImpl2025 {
    /// Fallback to 2021 delegate for base behavior.
    fn delegate_2021() -> &'static DynamicSchemePalettesDelegateImpl2021 {
        &DynamicSchemePalettesDelegateImpl2021
    }
}

impl DynamicSchemePalettesDelegate for DynamicSchemePalettesDelegateImpl2025 {
    fn get_primary_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette {
        let is_blue = Hct::is_blue(source.hue());

        match variant {
            Variant::Neutral => {
                let chroma = if platform == Platform::Phone {
                    if is_blue {
                        12.0
                    } else {
                        8.0
                    }
                } else {
                    if is_blue {
                        16.0
                    } else {
                        12.0
                    }
                };
                TonalPalette::from_hue_and_chroma(source.hue(), chroma)
            }
            Variant::TonalSpot => {
                let chroma = if platform == Platform::Phone && is_dark {
                    26.0
                } else {
                    32.0
                };
                TonalPalette::from_hue_and_chroma(source.hue(), chroma)
            }
            Variant::Expressive => {
                let chroma = if platform == Platform::Phone {
                    if is_dark {
                        36.0
                    } else {
                        48.0
                    }
                } else {
                    40.0
                };
                TonalPalette::from_hue_and_chroma(source.hue(), chroma)
            }
            Variant::Vibrant => {
                let chroma = if platform == Platform::Phone {
                    74.0
                } else {
                    56.0
                };
                TonalPalette::from_hue_and_chroma(source.hue(), chroma)
            }
            // Fall back to 2021 for other variants
            _ => Self::delegate_2021()
                .get_primary_palette(variant, source, is_dark, platform, contrast),
        }
    }

    fn get_secondary_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette {
        match variant {
            Variant::Neutral => {
                let is_blue = Hct::is_blue(source.hue());
                let chroma = if platform == Platform::Phone {
                    if is_blue { 6.0 } else { 4.0 }
                } else if is_blue {
                    10.0
                } else {
                    6.0
                };
                TonalPalette::from_hue_and_chroma(source.hue(), chroma)
            }
            Variant::TonalSpot => {
                TonalPalette::from_hue_and_chroma(source.hue(), 16.0)
            }
            Variant::Expressive => {
                const HUES: [f64; 9] = [0.0, 105.0, 140.0, 204.0, 253.0, 278.0, 300.0, 333.0, 360.0];
                const ROTATIONS: [f64; 8] = [-160.0, 155.0, -100.0, 96.0, -96.0, -156.0, -165.0, -160.0];
                let rotated_hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let chroma = if platform == Platform::Phone && is_dark { 16.0 } else { 24.0 };
                TonalPalette::from_hue_and_chroma(rotated_hue, chroma)
            }
            Variant::Vibrant => {
                const HUES: [f64; 6] = [0.0, 38.0, 105.0, 140.0, 333.0, 360.0];
                const ROTATIONS: [f64; 5] = [-14.0, 10.0, -14.0, 10.0, -14.0];
                let rotated_hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let chroma = if platform == Platform::Phone { 56.0 } else { 36.0 };
                TonalPalette::from_hue_and_chroma(rotated_hue, chroma)
            }
            // Fall back to 2021 for other variants
            _ => Self::delegate_2021()
                .get_secondary_palette(variant, source, is_dark, platform, contrast),
        }
    }

    fn get_tertiary_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette {
        match variant {
            Variant::Neutral => {
                const HUES: [f64; 8] = [0.0, 38.0, 105.0, 161.0, 204.0, 278.0, 333.0, 360.0];
                const ROTATIONS: [f64; 7] = [-32.0, 26.0, 10.0, -39.0, 24.0, -15.0, -32.0];
                let hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let chroma = if platform == Platform::Phone { 20.0 } else { 36.0 };
                TonalPalette::from_hue_and_chroma(hue, chroma)
            }
            Variant::TonalSpot => {
                const HUES: [f64; 6] = [0.0, 20.0, 71.0, 161.0, 333.0, 360.0];
                const ROTATIONS: [f64; 5] = [-40.0, 48.0, -32.0, 40.0, -32.0];
                let hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let chroma = if platform == Platform::Phone { 28.0 } else { 32.0 };
                TonalPalette::from_hue_and_chroma(hue, chroma)
            }
            Variant::Expressive => {
                const HUES: [f64; 9] = [0.0, 105.0, 140.0, 204.0, 253.0, 278.0, 300.0, 333.0, 360.0];
                const ROTATIONS: [f64; 8] = [-165.0, 160.0, -105.0, 101.0, -101.0, -160.0, -170.0, -165.0];
                let rotated_hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let chroma = 48.0;
                TonalPalette::from_hue_and_chroma(rotated_hue, chroma)
            }
            Variant::Vibrant => {
                const HUES: [f64; 9] = [0.0, 38.0, 71.0, 105.0, 140.0, 161.0, 253.0, 333.0, 360.0];
                const ROTATIONS: [f64; 8] = [-72.0, 35.0, 24.0, -24.0, 62.0, 50.0, 62.0, -72.0];
                let rotated_hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let chroma = 56.0;
                TonalPalette::from_hue_and_chroma(rotated_hue, chroma)
            }
            // Fall back to 2021 for other variants
            _ => Self::delegate_2021()
                .get_tertiary_palette(variant, source, is_dark, platform, contrast),
        }
    }

    fn get_neutral_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette {
        match variant {
            Variant::Neutral => TonalPalette::from_hue_and_chroma(
                source.hue(),
                if platform == Platform::Phone { 1.4 } else { 6.0 },
            ),
            Variant::TonalSpot => TonalPalette::from_hue_and_chroma(
                source.hue(),
                if platform == Platform::Phone { 5.0 } else { 10.0 },
            ),
            Variant::Expressive => {
                const HUES: [f64; 7] = [0.0, 71.0, 124.0, 253.0, 278.0, 300.0, 360.0];
                const ROTATIONS: [f64; 6] = [10.0, 0.0, 10.0, 0.0, 10.0, 0.0];
                let hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let chroma = if platform == Platform::Phone {
                    if is_dark {
                        if Hct::is_yellow(hue) { 6.0 } else { 14.0 }
                    } else {
                        18.0
                    }
                } else {
                    12.0
                };
                TonalPalette::from_hue_and_chroma(hue, chroma)
            }
            Variant::Vibrant => {
                const HUES: [f64; 6] = [0.0, 38.0, 105.0, 140.0, 333.0, 360.0];
                const ROTATIONS: [f64; 5] = [-14.0, 10.0, -14.0, 10.0, -14.0];
                let hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let chroma = if platform == Platform::Phone {
                    28.0
                } else if Hct::is_blue(hue) {
                    28.0
                } else {
                    20.0
                };
                TonalPalette::from_hue_and_chroma(hue, chroma)
            }
            // Fall back to 2021 for other variants
            _ => Self::delegate_2021()
                .get_neutral_palette(variant, source, is_dark, platform, contrast),
        }
    }

    fn get_neutral_variant_palette(
        &self,
        variant: Variant,
        source: &Hct,
        is_dark: bool,
        platform: Platform,
        contrast: f64,
    ) -> TonalPalette {
        match variant {
            Variant::Neutral => TonalPalette::from_hue_and_chroma(
                source.hue(),
                (if platform == Platform::Phone { 1.4 } else { 6.0 }) * 2.2,
            ),
            Variant::TonalSpot => {
                TonalPalette::from_hue_and_chroma(
                    source.hue(),
                    (if platform == Platform::Phone { 5.0 } else { 10.0 }) * 1.7,
                )
            }
            Variant::Expressive => {
                const HUES: [f64; 7] = [0.0, 71.0, 124.0, 253.0, 278.0, 300.0, 360.0];
                const ROTATIONS: [f64; 6] = [10.0, 0.0, 10.0, 0.0, 10.0, 0.0];
                let hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let neutral_chroma = if platform == Platform::Phone {
                    if is_dark {
                        if Hct::is_yellow(hue) { 6.0 } else { 14.0 }
                    } else {
                        18.0
                    }
                } else {
                    12.0
                };
                let multiplier = if (105.0..125.0).contains(&hue) { 1.6 } else { 2.3 };
                TonalPalette::from_hue_and_chroma(hue, neutral_chroma * multiplier)
            }
            Variant::Vibrant => {
                const HUES: [f64; 6] = [0.0, 38.0, 105.0, 140.0, 333.0, 360.0];
                const ROTATIONS: [f64; 5] = [-14.0, 10.0, -14.0, 10.0, -14.0];
                let hue = get_rotated_hue(source, &HUES, &ROTATIONS);
                let neutral_chroma = if platform == Platform::Phone {
                    28.0
                } else if Hct::is_blue(hue) {
                    28.0
                } else {
                    20.0
                };
                TonalPalette::from_hue_and_chroma(hue, neutral_chroma * 1.29)
            }
            // Fall back to 2021 for other variants
            _ => Self::delegate_2021()
                .get_neutral_variant_palette(variant, source, is_dark, platform, contrast),
        }
    }

    fn get_error_palette(
        &self,
        variant: Variant,
        source: &Hct,
        _is_dark: bool,
        platform: Platform,
        _contrast: f64,
    ) -> Option<TonalPalette> {
        const ERROR_BREAKPOINTS: [f64; 9] =
            [0.0, 3.0, 13.0, 23.0, 33.0, 43.0, 153.0, 273.0, 360.0];
        const ERROR_ROTATIONS: [f64; 8] = [12.0, 22.0, 32.0, 12.0, 22.0, 32.0, 22.0, 12.0];
        let error_hue = get_piecewise_value(
            source.hue(),
            &ERROR_BREAKPOINTS,
            &ERROR_ROTATIONS,
        );
        match variant {
            Variant::Neutral => Some(TonalPalette::from_hue_and_chroma(
                error_hue,
                if platform == Platform::Phone { 50.0 } else { 40.0 },
            )),
            Variant::TonalSpot => Some(TonalPalette::from_hue_and_chroma(
                error_hue,
                if platform == Platform::Phone { 60.0 } else { 48.0 },
            )),
            Variant::Expressive => Some(TonalPalette::from_hue_and_chroma(
                error_hue,
                if platform == Platform::Phone { 64.0 } else { 48.0 },
            )),
            Variant::Vibrant => Some(TonalPalette::from_hue_and_chroma(
                error_hue,
                if platform == Platform::Phone { 80.0 } else { 60.0 },
            )),
            _ => None,
        }
    }
}

// <FILE>crates/mcu-dynamiccolor/src/impl_palettes_2025.rs</FILE> - <DESC>2025 spec palette generation delegate implementation</DESC>
// <VERS>END OF VERSION: 1.0.0</VERS>
