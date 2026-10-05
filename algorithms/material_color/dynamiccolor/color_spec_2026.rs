/**
 * @license
 * Copyright 2026 Google LLC
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

// Google Material Color Utilities 2026 dynamic-color delegate.
//
// The canonical 2026 delegate extends 2025 and specializes CMF. Scheme
// fallback ensures this delegate is selected only for CMF requests; unchanged
// roles deliberately delegate to the canonical 2025 definitions.

use crate::material_color::hct::Hct;
use crate::material_color::palettes::TonalPalette;
use crate::material_color::roles::MaterialRole;
use crate::material_color::utils::math::clamp_double;
use std::sync::Arc;

use super::{
    ColorSpecDelegate, ColorSpecDelegateImpl2025, ContrastCurve, DeltaConstraint, DynamicColor,
    DynamicScheme, Platform, ToneDeltaPair, TonePolarity,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct ColorSpecDelegateImpl2026;

type PaletteFn = fn(&DynamicScheme) -> TonalPalette;
type ToneFn = fn(&DynamicScheme) -> f64;
type ChromaFn = fn(&DynamicScheme) -> f64;
type BackgroundFn = fn(&DynamicScheme) -> Option<DynamicColor>;
type CurveFn = fn(&DynamicScheme) -> Option<ContrastCurve>;
type DeltaFn = fn(&DynamicScheme) -> Option<ToneDeltaPair>;

#[allow(clippy::too_many_arguments)]
fn make_color(
    name: &'static str,
    palette: PaletteFn,
    tone: Option<ToneFn>,
    is_background: bool,
    chroma_multiplier: Option<ChromaFn>,
    background: Option<BackgroundFn>,
    second_background: Option<BackgroundFn>,
    contrast_curve: Option<CurveFn>,
    tone_delta_pair: Option<DeltaFn>,
) -> DynamicColor {
    DynamicColor::from_palette(
        name,
        palette,
        tone,
        is_background,
        chroma_multiplier,
        background,
        second_background,
        contrast_curve,
        tone_delta_pair,
    )
}

fn primary_palette(s: &DynamicScheme) -> TonalPalette {
    s.primary_palette.clone()
}
fn secondary_palette(s: &DynamicScheme) -> TonalPalette {
    s.secondary_palette.clone()
}
fn tertiary_palette(s: &DynamicScheme) -> TonalPalette {
    s.tertiary_palette.clone()
}
fn neutral_palette(s: &DynamicScheme) -> TonalPalette {
    s.neutral_palette.clone()
}
fn error_palette(s: &DynamicScheme) -> TonalPalette {
    s.error_palette.clone()
}

fn highest_surface(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(if s.is_dark {
        ColorSpecDelegateImpl2026.surface_bright()
    } else {
        ColorSpecDelegateImpl2026.surface_dim()
    })
}
fn inverse_surface(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(ColorSpecDelegateImpl2026.inverse_surface())
}
fn primary_background(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(ColorSpecDelegateImpl2026.primary())
}
fn primary_container_background(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(ColorSpecDelegateImpl2026.primary_container())
}
fn primary_fixed_background(s: &DynamicScheme) -> Option<DynamicColor> {
    let fixed = ColorSpecDelegateImpl2026.primary_fixed();
    let dim = ColorSpecDelegateImpl2026.primary_fixed_dim();
    Some(if fixed.get_tone(s) > 57.0 { dim } else { fixed })
}
fn secondary_background(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(ColorSpecDelegateImpl2026.secondary())
}
fn secondary_container_background(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(ColorSpecDelegateImpl2026.secondary_container())
}
fn secondary_fixed_background(s: &DynamicScheme) -> Option<DynamicColor> {
    let fixed = ColorSpecDelegateImpl2026.secondary_fixed();
    let dim = ColorSpecDelegateImpl2026.secondary_fixed_dim();
    Some(if fixed.get_tone(s) > 57.0 { dim } else { fixed })
}
fn tertiary_background(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(ColorSpecDelegateImpl2026.tertiary())
}
fn tertiary_container_background(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(ColorSpecDelegateImpl2026.tertiary_container())
}
fn tertiary_fixed_background(s: &DynamicScheme) -> Option<DynamicColor> {
    let fixed = ColorSpecDelegateImpl2026.tertiary_fixed();
    let dim = ColorSpecDelegateImpl2026.tertiary_fixed_dim();
    Some(if fixed.get_tone(s) > 57.0 { dim } else { fixed })
}
fn error_background(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(ColorSpecDelegateImpl2026.error())
}
fn error_container_background(s: &DynamicScheme) -> Option<DynamicColor> {
    Some(ColorSpecDelegateImpl2026.error_container())
}

fn contrast_curve(low: f64, normal: f64, medium: f64, high: f64) -> ContrastCurve {
    ContrastCurve::new(low, normal, medium, high)
}
fn get_curve(default_contrast: f64) -> ContrastCurve {
    if default_contrast == 1.5 {
        contrast_curve(1.5, 1.5, 3.0, 5.5)
    } else if default_contrast == 3.0 {
        contrast_curve(3.0, 3.0, 4.5, 7.0)
    } else if default_contrast == 4.5 {
        contrast_curve(4.5, 4.5, 7.0, 11.0)
    } else if default_contrast == 6.0 {
        contrast_curve(6.0, 6.0, 7.0, 11.0)
    } else if default_contrast == 7.0 {
        contrast_curve(7.0, 7.0, 11.0, 21.0)
    } else if default_contrast == 9.0 {
        contrast_curve(9.0, 9.0, 11.0, 21.0)
    } else if default_contrast == 11.0 {
        contrast_curve(11.0, 11.0, 21.0, 21.0)
    } else if default_contrast == 21.0 {
        contrast_curve(21.0, 21.0, 21.0, 21.0)
    } else {
        contrast_curve(default_contrast, default_contrast, 7.0, 21.0)
    }
}
fn curve_1_5(_: &DynamicScheme) -> Option<ContrastCurve> {
    Some(get_curve(1.5))
}
fn curve_3(_: &DynamicScheme) -> Option<ContrastCurve> {
    Some(get_curve(3.0))
}
fn curve_4_5(_: &DynamicScheme) -> Option<ContrastCurve> {
    Some(get_curve(4.5))
}
fn curve_6(_: &DynamicScheme) -> Option<ContrastCurve> {
    Some(get_curve(6.0))
}
fn curve_7(_: &DynamicScheme) -> Option<ContrastCurve> {
    Some(get_curve(7.0))
}
fn curve_9(s: &DynamicScheme) -> Option<ContrastCurve> {
    Some(get_curve(if s.is_dark { 11.0 } else { 9.0 }))
}
fn curve_if_positive_1_5(s: &DynamicScheme) -> Option<ContrastCurve> {
    (s.contrast_level > 0.0).then(|| get_curve(1.5))
}

fn cmf_surface_tone(s: &DynamicScheme) -> f64 {
    if s.is_dark { 4.0 } else { 98.0 }
}
fn cmf_surface_dim_tone(s: &DynamicScheme) -> f64 {
    if s.is_dark { 4.0 } else { 87.0 }
}
fn cmf_surface_dim_chroma(s: &DynamicScheme) -> f64 {
    if s.is_dark { 1.0 } else { 1.7 }
}
fn cmf_surface_bright_tone(s: &DynamicScheme) -> f64 {
    if s.is_dark { 18.0 } else { 98.0 }
}
fn cmf_surface_bright_chroma(s: &DynamicScheme) -> f64 {
    if s.is_dark { 1.7 } else { 1.0 }
}
fn cmf_surface_container_lowest(s: &DynamicScheme) -> f64 {
    if s.is_dark { 0.0 } else { 100.0 }
}
fn cmf_surface_container_low(s: &DynamicScheme) -> f64 {
    if s.is_dark { 6.0 } else { 96.0 }
}
fn cmf_surface_container(s: &DynamicScheme) -> f64 {
    if s.is_dark { 9.0 } else { 94.0 }
}
fn cmf_surface_container_high(s: &DynamicScheme) -> f64 {
    if s.is_dark { 12.0 } else { 92.0 }
}
fn cmf_surface_container_highest(s: &DynamicScheme) -> f64 {
    if s.is_dark { 15.0 } else { 90.0 }
}
fn cmf_neutral_chroma_1_25(_: &DynamicScheme) -> f64 {
    1.25
}
fn cmf_neutral_chroma_1_4(_: &DynamicScheme) -> f64 {
    1.4
}
fn cmf_neutral_chroma_1_5(_: &DynamicScheme) -> f64 {
    1.5
}
fn cmf_neutral_chroma_1_7(_: &DynamicScheme) -> f64 {
    1.7
}

fn cmf_primary_tone(s: &DynamicScheme) -> f64 {
    if s.source_color_hct.chroma() <= 12.0 {
        if s.is_dark { 80.0 } else { 40.0 }
    } else {
        s.source_color_hct.tone()
    }
}
fn cmf_primary_container_tone(s: &DynamicScheme) -> f64 {
    let source = s.source_color_hct;
    if !s.is_dark && source.chroma() <= 12.0 {
        90.0
    } else if source.tone() > 55.0 {
        clamp_double(61.0, 90.0, source.tone())
    } else {
        clamp_double(30.0, 49.0, source.tone())
    }
}
fn cmf_primary_container_curve(s: &DynamicScheme) -> Option<ContrastCurve> {
    (s.contrast_level > 0.0).then(|| get_curve(1.5))
}
fn primary_delta_pair(s: &DynamicScheme) -> Option<ToneDeltaPair> {
    (s.platform == Platform::Phone).then(|| {
        ToneDeltaPair::new(
            ColorSpecDelegateImpl2026.primary_container(),
            ColorSpecDelegateImpl2026.primary(),
            5.0,
            TonePolarity::RelativeLighter,
            true,
            DeltaConstraint::Farther,
        )
    })
}
fn fixed_polarity_pair(
    lower: DynamicColor,
    upper: DynamicColor,
) -> ToneDeltaPair {
    ToneDeltaPair::new(lower, upper, 5.0, TonePolarity::Darker, true, DeltaConstraint::Exact)
}
fn primary_fixed_tone(s: &DynamicScheme) -> f64 {
    let mut fixed = s.clone();
    fixed.is_dark = false;
    fixed.contrast_level = 0.0;
    ColorSpecDelegateImpl2026.primary_container().get_tone(&fixed)
}
fn primary_fixed_dim_tone(s: &DynamicScheme) -> f64 {
    ColorSpecDelegateImpl2026.primary_fixed().get_tone(s)
}
fn primary_fixed_delta(_: &DynamicScheme) -> Option<ToneDeltaPair> {
    Some(fixed_polarity_pair(
        ColorSpecDelegateImpl2026.primary_fixed_dim(),
        ColorSpecDelegateImpl2026.primary_fixed(),
    ))
}

fn cmf_secondary_tone(s: &DynamicScheme) -> f64 {
    if s.is_dark {
        t_min_c(&s.secondary_palette, 0.0, 100.0)
    } else {
        t_max_c(&s.secondary_palette, 0.0, 100.0, 1.0)
    }
}
fn cmf_secondary_container_tone(s: &DynamicScheme) -> f64 {
    if s.is_dark {
        t_min_c(&s.secondary_palette, 20.0, 49.0)
    } else {
        t_max_c(&s.secondary_palette, 61.0, 90.0, 1.0)
    }
}
fn secondary_delta_pair(s: &DynamicScheme) -> Option<ToneDeltaPair> {
    (s.platform == Platform::Phone).then(|| {
        ToneDeltaPair::new(
            ColorSpecDelegateImpl2026.secondary_container(),
            ColorSpecDelegateImpl2026.secondary(),
            5.0,
            TonePolarity::RelativeLighter,
            true,
            DeltaConstraint::Farther,
        )
    })
}
fn cmf_secondary_container_curve(s: &DynamicScheme) -> Option<ContrastCurve> {
    (s.contrast_level > 0.0).then(|| get_curve(1.5))
}
fn secondary_fixed_tone(s: &DynamicScheme) -> f64 {
    let mut fixed = s.clone();
    fixed.is_dark = false;
    fixed.contrast_level = 0.0;
    ColorSpecDelegateImpl2026.secondary_container().get_tone(&fixed)
}
fn secondary_fixed_dim_tone(s: &DynamicScheme) -> f64 {
    ColorSpecDelegateImpl2026.secondary_fixed().get_tone(s)
}
fn secondary_fixed_delta(_: &DynamicScheme) -> Option<ToneDeltaPair> {
    Some(fixed_polarity_pair(
        ColorSpecDelegateImpl2026.secondary_fixed_dim(),
        ColorSpecDelegateImpl2026.secondary_fixed(),
    ))
}

fn cmf_tertiary_tone(s: &DynamicScheme) -> f64 {
    s.source_color_hcts
        .get(1)
        .copied()
        .unwrap_or(s.source_color_hct)
        .tone()
}
fn cmf_tertiary_container_tone(s: &DynamicScheme) -> f64 {
    let source = s.source_color_hcts.get(1).copied().unwrap_or(s.source_color_hct);
    if source.tone() > 55.0 {
        clamp_double(61.0, 90.0, source.tone())
    } else {
        clamp_double(20.0, 49.0, source.tone())
    }
}
fn tertiary_delta_pair(s: &DynamicScheme) -> Option<ToneDeltaPair> {
    (s.platform == Platform::Phone).then(|| {
        ToneDeltaPair::new(
            ColorSpecDelegateImpl2026.tertiary_container(),
            ColorSpecDelegateImpl2026.tertiary(),
            5.0,
            TonePolarity::RelativeLighter,
            true,
            DeltaConstraint::Farther,
        )
    })
}
fn cmf_tertiary_container_curve(s: &DynamicScheme) -> Option<ContrastCurve> {
    (s.contrast_level > 0.0).then(|| get_curve(1.5))
}
fn tertiary_fixed_tone(s: &DynamicScheme) -> f64 {
    let mut fixed = s.clone();
    fixed.is_dark = false;
    fixed.contrast_level = 0.0;
    ColorSpecDelegateImpl2026.tertiary_container().get_tone(&fixed)
}
fn tertiary_fixed_dim_tone(s: &DynamicScheme) -> f64 {
    ColorSpecDelegateImpl2026.tertiary_fixed().get_tone(s)
}
fn tertiary_fixed_delta(_: &DynamicScheme) -> Option<ToneDeltaPair> {
    Some(fixed_polarity_pair(
        ColorSpecDelegateImpl2026.tertiary_fixed_dim(),
        ColorSpecDelegateImpl2026.tertiary_fixed(),
    ))
}

fn cmf_error_tone(s: &DynamicScheme) -> f64 {
    t_max_c(&s.error_palette, 0.0, 100.0, 1.0)
}
fn cmf_error_container_tone(s: &DynamicScheme) -> f64 {
    if s.is_dark {
        t_min_c(&s.error_palette, 0.0, 100.0)
    } else {
        t_max_c(&s.error_palette, 0.0, 100.0, 1.0)
    }
}
fn error_delta_pair(s: &DynamicScheme) -> Option<ToneDeltaPair> {
    (s.platform == Platform::Phone).then(|| {
        ToneDeltaPair::new(
            ColorSpecDelegateImpl2026.error_container(),
            ColorSpecDelegateImpl2026.error(),
            5.0,
            TonePolarity::RelativeLighter,
            true,
            DeltaConstraint::Farther,
        )
    })
}
fn cmf_error_container_curve(s: &DynamicScheme) -> Option<ContrastCurve> {
    (s.contrast_level > 0.0).then(|| get_curve(1.5))
}

fn t_max_c(palette: &TonalPalette, lower: f64, upper: f64, multiplier: f64) -> f64 {
    clamp_double(
        lower,
        upper,
        find_best_tone_for_chroma(palette.hue(), palette.chroma() * multiplier, 100.0, true),
    )
}
fn t_min_c(palette: &TonalPalette, lower: f64, upper: f64) -> f64 {
    clamp_double(
        lower,
        upper,
        find_best_tone_for_chroma(palette.hue(), palette.chroma(), 0.0, false),
    )
}
fn find_best_tone_for_chroma(hue: f64, chroma: f64, mut tone: f64, decreasing: bool) -> f64 {
    let mut answer = tone;
    let mut best = Hct::from(hue, chroma, answer);
    while best.chroma() < chroma {
        if tone < 0.0 || tone > 100.0 {
            break;
        }
        tone += if decreasing { -1.0 } else { 1.0 };
        let candidate = Hct::from(hue, chroma, tone);
        if best.chroma() < candidate.chroma() {
            best = candidate;
            answer = tone;
        }
    }
    answer
}

impl ColorSpecDelegateImpl2026 {
    fn for_role(role: MaterialRole) -> DynamicColor {
        if let Some(color) = Self::cmf_override(role) {
            color
        } else {
            Self::inherited(role)
        }
    }

    fn cmf_override(role: MaterialRole) -> Option<DynamicColor> {
        Some(match role {
            MaterialRole::Surface => make_color(
                "surface", neutral_palette, Some(cmf_surface_tone), true, None, None, None, None, None,
            ),
            MaterialRole::SurfaceDim => make_color(
                "surface_dim", neutral_palette, Some(cmf_surface_dim_tone), true,
                Some(cmf_surface_dim_chroma), None, None, None, None,
            ),
            MaterialRole::SurfaceBright => make_color(
                "surface_bright", neutral_palette, Some(cmf_surface_bright_tone), true,
                Some(cmf_surface_bright_chroma), None, None, None, None,
            ),
            MaterialRole::SurfaceContainerLowest => make_color(
                "surface_container_lowest", neutral_palette, Some(cmf_surface_container_lowest),
                true, None, None, None, None, None,
            ),
            MaterialRole::SurfaceContainerLow => make_color(
                "surface_container_low", neutral_palette, Some(cmf_surface_container_low), true,
                Some(cmf_neutral_chroma_1_25), None, None, None, None,
            ),
            MaterialRole::SurfaceContainer => make_color(
                "surface_container", neutral_palette, Some(cmf_surface_container), true,
                Some(cmf_neutral_chroma_1_4), None, None, None, None,
            ),
            MaterialRole::SurfaceContainerHigh => make_color(
                "surface_container_high", neutral_palette, Some(cmf_surface_container_high), true,
                Some(cmf_neutral_chroma_1_5), None, None, None, None,
            ),
            MaterialRole::SurfaceContainerHighest => make_color(
                "surface_container_highest", neutral_palette, Some(cmf_surface_container_highest),
                true, Some(cmf_neutral_chroma_1_7), None, None, None, None,
            ),
            MaterialRole::OnSurface => make_color(
                "on_surface", neutral_palette, None, false, Some(cmf_neutral_chroma_1_7),
                Some(highest_surface), None, Some(curve_9), None,
            ),
            MaterialRole::OnSurfaceVariant => make_color(
                "on_surface_variant", neutral_palette, None, false,
                Some(cmf_neutral_chroma_1_7), Some(highest_surface), None,
                Some(|s| Some(get_curve(if s.is_dark { 6.0 } else { 4.5 }))), None,
            ),
            MaterialRole::Outline => make_color(
                "outline", neutral_palette, None, false, Some(cmf_neutral_chroma_1_7),
                Some(highest_surface), None, Some(curve_3), None,
            ),
            MaterialRole::OutlineVariant => make_color(
                "outline_variant", neutral_palette, None, false,
                Some(cmf_neutral_chroma_1_7), Some(highest_surface), None,
                Some(curve_1_5), None,
            ),
            MaterialRole::InverseSurface => make_color(
                "inverse_surface", neutral_palette,
                Some(|s| if s.is_dark { 98.0 } else { 4.0 }), true,
                Some(cmf_neutral_chroma_1_7), None, None, None, None,
            ),
            MaterialRole::InverseOnSurface => make_color(
                "inverse_on_surface", neutral_palette, None, false, None,
                Some(inverse_surface), None, Some(curve_7), None,
            ),
            MaterialRole::SurfaceVariant => {
                Self::renamed(Self::for_role(MaterialRole::SurfaceContainerHighest), "surface_variant")
            }
            MaterialRole::SurfaceTint => {
                Self::renamed(Self::for_role(MaterialRole::Primary), "surface_tint")
            }
            MaterialRole::Primary => make_color(
                "primary", primary_palette, Some(cmf_primary_tone), true, None,
                Some(highest_surface), None, Some(curve_4_5), Some(primary_delta_pair),
            ),
            MaterialRole::OnPrimary => make_color(
                "on_primary", primary_palette, None, false, None,
                Some(primary_background), None, Some(curve_6), None,
            ),
            MaterialRole::PrimaryContainer => make_color(
                "primary_container", primary_palette, Some(cmf_primary_container_tone), true,
                None, Some(highest_surface), None, Some(cmf_primary_container_curve), None,
            ),
            MaterialRole::OnPrimaryContainer => make_color(
                "on_primary_container", primary_palette, None, false, None,
                Some(primary_container_background), None, Some(curve_6), None,
            ),
            MaterialRole::PrimaryFixed => make_color(
                "primary_fixed", primary_palette, Some(primary_fixed_tone), true, None,
                Some(highest_surface), None, Some(curve_if_positive_1_5), None,
            ),
            MaterialRole::PrimaryFixedDim => make_color(
                "primary_fixed_dim", primary_palette, Some(primary_fixed_dim_tone), true, None,
                Some(highest_surface), None, Some(curve_if_positive_1_5), Some(primary_fixed_delta),
            ),
            MaterialRole::OnPrimaryFixed => make_color(
                "on_primary_fixed", primary_palette, None, false, None,
                Some(primary_fixed_background), None, Some(curve_7), None,
            ),
            MaterialRole::OnPrimaryFixedVariant => make_color(
                "on_primary_fixed_variant", primary_palette, None, false, None,
                Some(primary_fixed_background), None, Some(curve_4_5), None,
            ),
            MaterialRole::PrimaryDim => Self::renamed(Self::for_role(MaterialRole::Primary), "primary_dim"),
            MaterialRole::Secondary => make_color(
                "secondary", secondary_palette, Some(cmf_secondary_tone), true, None,
                Some(highest_surface), None, Some(curve_4_5), Some(secondary_delta_pair),
            ),
            MaterialRole::OnSecondary => make_color(
                "on_secondary", secondary_palette, None, false, None,
                Some(secondary_background), None, Some(curve_6), None,
            ),
            MaterialRole::SecondaryContainer => make_color(
                "secondary_container", secondary_palette, Some(cmf_secondary_container_tone), true,
                None, Some(highest_surface), None, Some(cmf_secondary_container_curve), None,
            ),
            MaterialRole::OnSecondaryContainer => make_color(
                "on_secondary_container", secondary_palette, None, false, None,
                Some(secondary_container_background), None, Some(curve_6), None,
            ),
            MaterialRole::SecondaryFixed => make_color(
                "secondary_fixed", secondary_palette, Some(secondary_fixed_tone), true, None,
                Some(highest_surface), None, Some(curve_if_positive_1_5), None,
            ),
            MaterialRole::SecondaryFixedDim => make_color(
                "secondary_fixed_dim", secondary_palette, Some(secondary_fixed_dim_tone), true,
                None, Some(highest_surface), None, Some(curve_if_positive_1_5),
                Some(secondary_fixed_delta),
            ),
            MaterialRole::OnSecondaryFixed => make_color(
                "on_secondary_fixed", secondary_palette, None, false, None,
                Some(secondary_fixed_background), None, Some(curve_7), None,
            ),
            MaterialRole::OnSecondaryFixedVariant => make_color(
                "on_secondary_fixed_variant", secondary_palette, None, false, None,
                Some(secondary_fixed_background), None, Some(curve_4_5), None,
            ),
            MaterialRole::SecondaryDim => Self::renamed(Self::for_role(MaterialRole::Secondary), "secondary_dim"),
            MaterialRole::Tertiary => make_color(
                "tertiary", tertiary_palette, Some(cmf_tertiary_tone), true, None,
                Some(highest_surface), None, Some(curve_4_5), Some(tertiary_delta_pair),
            ),
            MaterialRole::OnTertiary => make_color(
                "on_tertiary", tertiary_palette, None, false, None,
                Some(tertiary_background), None, Some(curve_6), None,
            ),
            MaterialRole::TertiaryContainer => make_color(
                "tertiary_container", tertiary_palette, Some(cmf_tertiary_container_tone), true,
                None, Some(highest_surface), None, Some(cmf_tertiary_container_curve), None,
            ),
            MaterialRole::OnTertiaryContainer => make_color(
                "on_tertiary_container", tertiary_palette, None, false, None,
                Some(tertiary_container_background), None, Some(curve_6), None,
            ),
            MaterialRole::TertiaryFixed => make_color(
                "tertiary_fixed", tertiary_palette, Some(tertiary_fixed_tone), true, None,
                Some(highest_surface), None, Some(curve_if_positive_1_5), None,
            ),
            MaterialRole::TertiaryFixedDim => make_color(
                "tertiary_fixed_dim", tertiary_palette, Some(tertiary_fixed_dim_tone), true,
                None, Some(highest_surface), None, Some(curve_if_positive_1_5),
                Some(tertiary_fixed_delta),
            ),
            MaterialRole::OnTertiaryFixed => make_color(
                "on_tertiary_fixed", tertiary_palette, None, false, None,
                Some(tertiary_fixed_background), None, Some(curve_7), None,
            ),
            MaterialRole::OnTertiaryFixedVariant => make_color(
                "on_tertiary_fixed_variant", tertiary_palette, None, false, None,
                Some(tertiary_fixed_background), None, Some(curve_4_5), None,
            ),
            MaterialRole::TertiaryDim => Self::renamed(Self::for_role(MaterialRole::Tertiary), "tertiary_dim"),
            MaterialRole::Error => make_color(
                "error", error_palette, Some(cmf_error_tone), true, None,
                Some(highest_surface), None, Some(curve_4_5), Some(error_delta_pair),
            ),
            MaterialRole::OnError => make_color(
                "on_error", error_palette, None, false, None,
                Some(error_background), None, Some(curve_6), None,
            ),
            MaterialRole::ErrorContainer => make_color(
                "error_container", error_palette, Some(cmf_error_container_tone), true,
                None, Some(highest_surface), None, Some(cmf_error_container_curve), None,
            ),
            MaterialRole::OnErrorContainer => make_color(
                "on_error_container", error_palette, None, false, None,
                Some(error_container_background), None, Some(curve_6), None,
            ),
            MaterialRole::ErrorDim => Self::renamed(Self::for_role(MaterialRole::Error), "error_dim"),
            _ => return None,
        })
    }

    fn renamed(mut color: DynamicColor, name: &'static str) -> DynamicColor {
        color.name = name.to_owned();
        color
    }

    fn inherited(role: MaterialRole) -> DynamicColor {
        let base = ColorSpecDelegateImpl2025;
        match role {
            MaterialRole::PrimaryPaletteKeyColor => base.primary_palette_key_color(),
            MaterialRole::SecondaryPaletteKeyColor => base.secondary_palette_key_color(),
            MaterialRole::TertiaryPaletteKeyColor => base.tertiary_palette_key_color(),
            MaterialRole::NeutralPaletteKeyColor => base.neutral_palette_key_color(),
            MaterialRole::NeutralVariantPaletteKeyColor => base.neutral_variant_palette_key_color(),
            MaterialRole::ErrorPaletteKeyColor => base.error_palette_key_color(),
            MaterialRole::Background => {
                Self::renamed(Self::for_role(MaterialRole::Surface), "background")
            }
            MaterialRole::OnBackground => {
                let mut on_background = Self::for_role(MaterialRole::OnSurface);
                on_background.name = "on_background".to_owned();
                let on_surface = on_background.clone();
                on_background.tone = Arc::new(move |s: &DynamicScheme| {
                    if s.platform == Platform::Watch {
                        100.0
                    } else {
                        on_surface.get_tone(s)
                    }
                });
                on_background
            }
            MaterialRole::InversePrimary => {
                let mut inverse_primary = base.inverse_primary();
                inverse_primary.background = Some(Arc::new(|_s: &DynamicScheme| {
                    Some(Self::for_role(MaterialRole::InverseSurface))
                }));
                inverse_primary
            }
            MaterialRole::Shadow => base.shadow(),
            MaterialRole::Scrim => base.scrim(),
            _ => unreachable!("role must be inherited or a 2026 CMF override"),
        }
    }
}

macro_rules! color_spec_roles {
    ($($method:ident => $role:ident),* $(,)?) => {
        $(fn $method(&self) -> DynamicColor {
            Self::for_role(MaterialRole::$role)
        })*
    };
}

impl ColorSpecDelegate for ColorSpecDelegateImpl2026 {
    color_spec_roles! {
        primary_palette_key_color => PrimaryPaletteKeyColor,
        secondary_palette_key_color => SecondaryPaletteKeyColor,
        tertiary_palette_key_color => TertiaryPaletteKeyColor,
        neutral_palette_key_color => NeutralPaletteKeyColor,
        neutral_variant_palette_key_color => NeutralVariantPaletteKeyColor,
        error_palette_key_color => ErrorPaletteKeyColor,
        background => Background,
        on_background => OnBackground,
        surface => Surface,
        surface_dim => SurfaceDim,
        surface_bright => SurfaceBright,
        surface_container_lowest => SurfaceContainerLowest,
        surface_container_low => SurfaceContainerLow,
        surface_container => SurfaceContainer,
        surface_container_high => SurfaceContainerHigh,
        surface_container_highest => SurfaceContainerHighest,
        on_surface => OnSurface,
        surface_variant => SurfaceVariant,
        on_surface_variant => OnSurfaceVariant,
        inverse_surface => InverseSurface,
        inverse_on_surface => InverseOnSurface,
        outline => Outline,
        outline_variant => OutlineVariant,
        shadow => Shadow,
        scrim => Scrim,
        surface_tint => SurfaceTint,
        primary => Primary,
        on_primary => OnPrimary,
        primary_container => PrimaryContainer,
        on_primary_container => OnPrimaryContainer,
        inverse_primary => InversePrimary,
        primary_fixed => PrimaryFixed,
        primary_fixed_dim => PrimaryFixedDim,
        on_primary_fixed => OnPrimaryFixed,
        on_primary_fixed_variant => OnPrimaryFixedVariant,
        secondary => Secondary,
        on_secondary => OnSecondary,
        secondary_container => SecondaryContainer,
        on_secondary_container => OnSecondaryContainer,
        secondary_fixed => SecondaryFixed,
        secondary_fixed_dim => SecondaryFixedDim,
        on_secondary_fixed => OnSecondaryFixed,
        on_secondary_fixed_variant => OnSecondaryFixedVariant,
        tertiary => Tertiary,
        on_tertiary => OnTertiary,
        tertiary_container => TertiaryContainer,
        on_tertiary_container => OnTertiaryContainer,
        tertiary_fixed => TertiaryFixed,
        tertiary_fixed_dim => TertiaryFixedDim,
        on_tertiary_fixed => OnTertiaryFixed,
        on_tertiary_fixed_variant => OnTertiaryFixedVariant,
        error => Error,
        on_error => OnError,
        error_container => ErrorContainer,
        on_error_container => OnErrorContainer,
    }

    fn primary_dim(&self) -> Option<DynamicColor> {
        Some(Self::for_role(MaterialRole::PrimaryDim))
    }
    fn secondary_dim(&self) -> Option<DynamicColor> {
        Some(Self::for_role(MaterialRole::SecondaryDim))
    }
    fn tertiary_dim(&self) -> Option<DynamicColor> {
        Some(Self::for_role(MaterialRole::TertiaryDim))
    }
    fn error_dim(&self) -> Option<DynamicColor> {
        Some(Self::for_role(MaterialRole::ErrorDim))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material_color::dynamiccolor::DynamicSchemeOptions;
    use crate::material_color::dynamiccolor::Variant;

    fn cmf_scheme(seed: u32, dark: bool, platform: Platform) -> DynamicScheme {
        let mut options = DynamicSchemeOptions::new(Hct::from_int(seed), Variant::Cmf, 0.0, dark);
        options.platform = Some(platform);
        options.spec_version = Some(super::super::SpecVersion::Spec2026);
        DynamicScheme::new(options)
    }

    #[test]
    fn cmf_surface_tones_and_dim_roles_are_2026() {
        let light = cmf_scheme(0xff6750a4, false, Platform::Phone);
        let dark = cmf_scheme(0xff6750a4, true, Platform::Phone);
        assert_eq!(ColorSpecDelegateImpl2026.surface().get_tone(&light), 98.0);
        assert_eq!(ColorSpecDelegateImpl2026.surface().get_tone(&dark), 4.0);
        assert_eq!(ColorSpecDelegateImpl2026.surface_dim().get_tone(&light), 87.0);
        assert_eq!(ColorSpecDelegateImpl2026.surface_bright().get_tone(&dark), 18.0);
        assert_eq!(ColorSpecDelegateImpl2026.primary_dim().unwrap().name, "primary_dim");
    }
}
