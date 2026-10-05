
//! 2021 Material Design specification palette generation delegate.
//!
//! This module contains the palette generation logic for the original
//! Material Design 3 specification (2021).

use crate::material::color::dislike::DislikeAnalyzer;
use crate::material::color::hct::Hct;
use crate::material::color::palettes::TonalPalette;
use crate::material::color::temperature::TemperatureCache;
use crate::material::color::utils::math::sanitize_degrees_double;

use super::dynamic_scheme_palettes::{
    get_rotated_hue, DynamicSchemePalettesDelegate, HUES, SECONDARY_ROTATIONS_EXPRESSIVE,
    TERTIARY_ROTATIONS_EXPRESSIVE, VIBRANT_SECONDARY_ROTATIONS, VIBRANT_TERTIARY_ROTATIONS,
};
use super::{Platform, Variant};

/// Palette generation delegate for the 2021 Material Design 3 specification.
pub struct DynamicSchemePalettesDelegateImpl2021;

impl DynamicSchemePalettesDelegate for DynamicSchemePalettesDelegateImpl2021 {
    fn get_primary_palette(
        &self,
        variant: Variant,
        source: &Hct,
        _is_dark: bool,
        _platform: Platform,
        _contrast: f64,
    ) -> TonalPalette {
        match variant {
            Variant::Content | Variant::Fidelity => {
                TonalPalette::from_hue_and_chroma(source.hue(), source.chroma())
            }
            Variant::FruitSalad => TonalPalette::from_hue_and_chroma(
                sanitize_degrees_double(source.hue() - 50.0),
                48.0,
            ),
            Variant::Monochrome => TonalPalette::from_hue_and_chroma(source.hue(), 0.0),
            Variant::Neutral => TonalPalette::from_hue_and_chroma(source.hue(), 12.0),
            Variant::Rainbow => TonalPalette::from_hue_and_chroma(source.hue(), 48.0),
            Variant::TonalSpot => TonalPalette::from_hue_and_chroma(source.hue(), 36.0),
            Variant::Expressive => TonalPalette::from_hue_and_chroma(
                sanitize_degrees_double(source.hue() + 240.0),
                40.0,
            ),
            Variant::Vibrant => TonalPalette::from_hue_and_chroma(source.hue(), 200.0),
            Variant::Cmf => unreachable!("CMF palettes are constructed by DynamicScheme"),
        }
    }

    fn get_secondary_palette(
        &self,
        variant: Variant,
        source: &Hct,
        _is_dark: bool,
        _platform: Platform,
        _contrast: f64,
    ) -> TonalPalette {
        match variant {
            Variant::Content => {
                let chroma = (source.chroma() - 32.0).max(source.chroma() * 0.5);
                TonalPalette::from_hue_and_chroma(source.hue(), chroma)
            }
            Variant::Fidelity => {
                let chroma = (source.chroma() - 32.0).max(source.chroma() * 0.5);
                TonalPalette::from_hue_and_chroma(source.hue(), chroma)
            }
            Variant::FruitSalad => TonalPalette::from_hue_and_chroma(
                sanitize_degrees_double(source.hue() - 50.0),
                36.0,
            ),
            Variant::Monochrome => TonalPalette::from_hue_and_chroma(source.hue(), 0.0),
            Variant::Neutral => TonalPalette::from_hue_and_chroma(source.hue(), 8.0),
            Variant::Rainbow => TonalPalette::from_hue_and_chroma(source.hue(), 16.0),
            Variant::TonalSpot => TonalPalette::from_hue_and_chroma(source.hue(), 16.0),
            Variant::Expressive => {
                let rotated_hue = get_rotated_hue(source, &HUES, &SECONDARY_ROTATIONS_EXPRESSIVE);
                TonalPalette::from_hue_and_chroma(rotated_hue, 24.0)
            }
            Variant::Vibrant => {
                let rotated_hue = get_rotated_hue(source, &HUES, &VIBRANT_SECONDARY_ROTATIONS);
                TonalPalette::from_hue_and_chroma(rotated_hue, 24.0)
            }
            Variant::Cmf => unreachable!("CMF palettes are constructed by DynamicScheme"),
        }
    }

    fn get_tertiary_palette(
        &self,
        variant: Variant,
        source: &Hct,
        _is_dark: bool,
        _platform: Platform,
        _contrast: f64,
    ) -> TonalPalette {
        match variant {
            Variant::Content => {
                // Use TemperatureCache to get analogous colors
                let mut cache = TemperatureCache::new(*source);
                let analogous = cache.analogous(3, 6);
                // Get the tertiary from analogous colors (typically index 2)
                if analogous.len() > 2 {
                    let tertiary_hct = analogous[2];
                    let fixed = DislikeAnalyzer::fix_if_disliked(&tertiary_hct);
                    TonalPalette::from_hct(fixed)
                } else {
                    TonalPalette::from_hue_and_chroma(
                        sanitize_degrees_double(source.hue() + 60.0),
                        source.chroma(),
                    )
                }
            }
            Variant::Fidelity => {
                // Use TemperatureCache complement
                let mut cache = TemperatureCache::new(*source);
                let complement = cache.complement();
                let fixed = DislikeAnalyzer::fix_if_disliked(&complement);
                TonalPalette::from_hct(fixed)
            }
            Variant::FruitSalad => TonalPalette::from_hue_and_chroma(source.hue(), 36.0),
            Variant::Monochrome => TonalPalette::from_hue_and_chroma(source.hue(), 0.0),
            Variant::Neutral => TonalPalette::from_hue_and_chroma(
                sanitize_degrees_double(source.hue() + 60.0),
                12.0,
            ),
            Variant::Rainbow | Variant::TonalSpot => TonalPalette::from_hue_and_chroma(
                sanitize_degrees_double(source.hue() + 60.0),
                24.0,
            ),
            Variant::Expressive => {
                let rotated_hue = get_rotated_hue(source, &HUES, &TERTIARY_ROTATIONS_EXPRESSIVE);
                TonalPalette::from_hue_and_chroma(rotated_hue, 32.0)
            }
            Variant::Vibrant => {
                let rotated_hue = get_rotated_hue(source, &HUES, &VIBRANT_TERTIARY_ROTATIONS);
                TonalPalette::from_hue_and_chroma(rotated_hue, 32.0)
            }
            Variant::Cmf => unreachable!("CMF palettes are constructed by DynamicScheme"),
        }
    }

    fn get_neutral_palette(
        &self,
        variant: Variant,
        source: &Hct,
        _is_dark: bool,
        _platform: Platform,
        _contrast: f64,
    ) -> TonalPalette {
        match variant {
            Variant::Content | Variant::Fidelity => {
                TonalPalette::from_hue_and_chroma(source.hue(), source.chroma() / 8.0)
            }
            Variant::FruitSalad => TonalPalette::from_hue_and_chroma(source.hue(), 10.0),
            Variant::Monochrome => TonalPalette::from_hue_and_chroma(source.hue(), 0.0),
            Variant::Neutral => TonalPalette::from_hue_and_chroma(source.hue(), 2.0),
            Variant::Rainbow => TonalPalette::from_hue_and_chroma(source.hue(), 0.0),
            Variant::TonalSpot => TonalPalette::from_hue_and_chroma(source.hue(), 6.0),
            Variant::Expressive => {
                TonalPalette::from_hue_and_chroma(sanitize_degrees_double(source.hue() + 15.0), 8.0)
            }
            Variant::Vibrant => TonalPalette::from_hue_and_chroma(source.hue(), 10.0),
            Variant::Cmf => unreachable!("CMF palettes are constructed by DynamicScheme"),
        }
    }

    fn get_neutral_variant_palette(
        &self,
        variant: Variant,
        source: &Hct,
        _is_dark: bool,
        _platform: Platform,
        _contrast: f64,
    ) -> TonalPalette {
        match variant {
            Variant::Content | Variant::Fidelity => {
                TonalPalette::from_hue_and_chroma(source.hue(), (source.chroma() / 8.0) + 4.0)
            }
            Variant::FruitSalad => TonalPalette::from_hue_and_chroma(source.hue(), 16.0),
            Variant::Monochrome => TonalPalette::from_hue_and_chroma(source.hue(), 0.0),
            Variant::Neutral => TonalPalette::from_hue_and_chroma(source.hue(), 2.0),
            Variant::Rainbow => TonalPalette::from_hue_and_chroma(source.hue(), 0.0),
            Variant::TonalSpot => TonalPalette::from_hue_and_chroma(source.hue(), 8.0),
            Variant::Expressive => TonalPalette::from_hue_and_chroma(
                sanitize_degrees_double(source.hue() + 15.0),
                12.0,
            ),
            Variant::Vibrant => TonalPalette::from_hue_and_chroma(source.hue(), 12.0),
            Variant::Cmf => unreachable!("CMF palettes are constructed by DynamicScheme"),
        }
    }

    fn get_error_palette(
        &self,
        _variant: Variant,
        _source: &Hct,
        _is_dark: bool,
        _platform: Platform,
        _contrast: f64,
    ) -> Option<TonalPalette> {
        // 2021 spec uses default error palette (25.0, 84.0)
        None
    }
}
