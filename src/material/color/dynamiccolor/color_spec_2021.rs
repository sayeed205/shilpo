
use super::{
    ColorSpecDelegate, ContrastCurve, DeltaConstraint, DynamicColor, DynamicScheme, ToneDeltaPair,
    TonePolarity, Variant,
};
use crate::material::color::dislike::DislikeAnalyzer;
use crate::material::color::hct::Hct;

/// Helper function to check if scheme is Fidelity or Content variant
fn is_fidelity(scheme: &DynamicScheme) -> bool {
    matches!(scheme.variant, Variant::Fidelity | Variant::Content)
}

/// Helper function to check if scheme is Monochrome variant
fn is_monochrome(scheme: &DynamicScheme) -> bool {
    scheme.variant == Variant::Monochrome
}

/// Find the closest tone that achieves the desired chroma.
///
/// This function searches for a tone that gets as close as possible to the target chroma
/// by iterating through tones in the specified direction.
fn find_desired_chroma_by_tone(hue: f64, chroma: f64, tone: f64, by_decreasing_tone: bool) -> f64 {
    let mut answer = tone;
    let mut closest_to_chroma = Hct::from(hue, chroma, tone);

    if closest_to_chroma.chroma() < chroma {
        let mut chroma_peak = closest_to_chroma.chroma();

        while closest_to_chroma.chroma() < chroma {
            answer += if by_decreasing_tone { -1.0 } else { 1.0 };

            let potential_solution = Hct::from(hue, chroma, answer);

            if chroma_peak > potential_solution.chroma() {
                break;
            }

            if (potential_solution.chroma() - chroma).abs() < 0.4 {
                break;
            }

            let potential_delta = (potential_solution.chroma() - chroma).abs();
            let current_delta = (closest_to_chroma.chroma() - chroma).abs();

            if potential_delta < current_delta {
                closest_to_chroma = potential_solution;
            }

            chroma_peak = chroma_peak.max(potential_solution.chroma());
        }
    }

    answer
}

/// Implementation of the 2021 Material Design 3 color specification.
///
/// This struct implements the ColorSpecDelegate trait with color definitions
/// from the 2021 version of the Material Design 3 specification.
#[derive(Clone, Copy)]
pub struct ColorSpecDelegateImpl2021;

impl ColorSpecDelegateImpl2021 {
    /// Create a new 2021 color spec delegate
    pub fn new() -> Self {
        ColorSpecDelegateImpl2021
    }

    /// Returns the highest surface color for a scheme.
    /// In dark mode, this is surfaceBright; in light mode, this is surfaceDim.
    fn highest_surface(&self, scheme: &DynamicScheme) -> DynamicColor {
        if scheme.is_dark {
            self.surface_bright()
        } else {
            self.surface_dim()
        }
    }
}

impl Default for ColorSpecDelegateImpl2021 {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorSpecDelegate for ColorSpecDelegateImpl2021 {
    // ================================================================
    // Main Palettes
    // ================================================================

    fn primary_palette_key_color(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "primary_palette_key_color",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| s.primary_palette.key_color().tone()),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn secondary_palette_key_color(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "secondary_palette_key_color",
            |s| s.secondary_palette.clone(),
            Some(|s: &DynamicScheme| s.secondary_palette.key_color().tone()),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn tertiary_palette_key_color(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "tertiary_palette_key_color",
            |s| s.tertiary_palette.clone(),
            Some(|s: &DynamicScheme| s.tertiary_palette.key_color().tone()),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn neutral_palette_key_color(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "neutral_palette_key_color",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| s.neutral_palette.key_color().tone()),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn neutral_variant_palette_key_color(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "neutral_variant_palette_key_color",
            |s| s.neutral_variant_palette.clone(),
            Some(|s: &DynamicScheme| s.neutral_variant_palette.key_color().tone()),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn error_palette_key_color(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "error_palette_key_color",
            |s| s.error_palette.clone(),
            Some(|s: &DynamicScheme| s.error_palette.key_color().tone()),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    // ================================================================
    // Surfaces [S]
    // ================================================================

    fn background(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "background",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 6.0 } else { 98.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn on_background(&self) -> DynamicColor {
        let background = self.background();
        DynamicColor::from_palette(
            "on_background",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 90.0 } else { 10.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(background.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 3.0, 4.5, 7.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 6.0 } else { 98.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface_dim(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface_dim",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| {
                if s.is_dark {
                    6.0
                } else {
                    ContrastCurve::new(87.0, 87.0, 80.0, 75.0).get(s.contrast_level)
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface_bright(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface_bright",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| {
                if s.is_dark {
                    ContrastCurve::new(24.0, 24.0, 29.0, 34.0).get(s.contrast_level)
                } else {
                    98.0
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface_container_lowest(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface_container_lowest",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| {
                if s.is_dark {
                    ContrastCurve::new(4.0, 4.0, 2.0, 0.0).get(s.contrast_level)
                } else {
                    100.0
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface_container_low(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface_container_low",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| {
                if s.is_dark {
                    ContrastCurve::new(10.0, 10.0, 11.0, 12.0).get(s.contrast_level)
                } else {
                    ContrastCurve::new(96.0, 96.0, 96.0, 95.0).get(s.contrast_level)
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface_container(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface_container",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| {
                if s.is_dark {
                    ContrastCurve::new(12.0, 12.0, 16.0, 20.0).get(s.contrast_level)
                } else {
                    ContrastCurve::new(94.0, 94.0, 92.0, 90.0).get(s.contrast_level)
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface_container_high(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface_container_high",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| {
                if s.is_dark {
                    ContrastCurve::new(17.0, 17.0, 21.0, 25.0).get(s.contrast_level)
                } else {
                    ContrastCurve::new(92.0, 92.0, 88.0, 85.0).get(s.contrast_level)
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface_container_highest(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface_container_highest",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| {
                if s.is_dark {
                    ContrastCurve::new(22.0, 22.0, 26.0, 30.0).get(s.contrast_level)
                } else {
                    ContrastCurve::new(90.0, 90.0, 84.0, 80.0).get(s.contrast_level)
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn on_surface(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "on_surface",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 90.0 } else { 10.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface_variant(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface_variant",
            |s| s.neutral_variant_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 30.0 } else { 90.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn on_surface_variant(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "on_surface_variant",
            |s| s.neutral_variant_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 80.0 } else { 30.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn inverse_surface(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "inverse_surface",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 90.0 } else { 20.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn inverse_on_surface(&self) -> DynamicColor {
        let inverse_surface = self.inverse_surface();
        DynamicColor::from_palette(
            "inverse_on_surface",
            |s| s.neutral_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 20.0 } else { 95.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(inverse_surface.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn outline(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "outline",
            |s| s.neutral_variant_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 60.0 } else { 50.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.5, 3.0, 4.5, 7.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn outline_variant(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "outline_variant",
            |s| s.neutral_variant_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 30.0 } else { 80.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn shadow(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "shadow",
            |s| s.neutral_palette.clone(),
            Some(|_s: &DynamicScheme| 0.0),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn scrim(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "scrim",
            |s| s.neutral_palette.clone(),
            Some(|_s: &DynamicScheme| 0.0),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn surface_tint(&self) -> DynamicColor {
        DynamicColor::from_palette(
            "surface_tint",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 80.0 } else { 40.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    // ================================================================
    // Primaries [P]
    // ================================================================

    fn primary(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "primary",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| {
                if is_monochrome(s) {
                    if s.is_dark {
                        100.0
                    } else {
                        0.0
                    }
                } else if s.is_dark {
                    80.0
                } else {
                    40.0
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 7.0))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.primary_container(),
                    ColorSpecDelegateImpl2021.primary(),
                    10.0,
                    TonePolarity::Nearer,
                    false,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_primary(&self) -> DynamicColor {
        let primary = self.primary();
        DynamicColor::from_palette(
            "on_primary",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| {
                if is_monochrome(s) {
                    if s.is_dark {
                        10.0
                    } else {
                        90.0
                    }
                } else if s.is_dark {
                    20.0
                } else {
                    100.0
                }
            }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(primary.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn primary_container(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "primary_container",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| {
                if is_fidelity(s) {
                    s.source_color_hct.tone()
                } else if is_monochrome(s) {
                    if s.is_dark {
                        85.0
                    } else {
                        25.0
                    }
                } else if s.is_dark {
                    30.0
                } else {
                    90.0
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.primary_container(),
                    ColorSpecDelegateImpl2021.primary(),
                    10.0,
                    TonePolarity::Nearer,
                    false,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_primary_container(&self) -> DynamicColor {
        let primary_container = self.primary_container();
        let primary_container_for_tone = primary_container.clone();
        DynamicColor::from_palette(
            "on_primary_container",
            |s| s.primary_palette.clone(),
            Some(move |s: &DynamicScheme| {
                if is_fidelity(s) {
                    DynamicColor::foreground_tone(
                        (primary_container_for_tone.tone)(s),
                        4.5,
                    )
                } else if is_monochrome(s) {
                    if s.is_dark {
                        0.0
                    } else {
                        100.0
                    }
                } else if s.is_dark {
                    90.0
                } else {
                    30.0
                }
            }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(primary_container.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn inverse_primary(&self) -> DynamicColor {
        let inverse_surface = self.inverse_surface();
        DynamicColor::from_palette(
            "inverse_primary",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 40.0 } else { 80.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(inverse_surface.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 7.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    // ================================================================
    // Secondaries [Q]
    // ================================================================

    fn secondary(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "secondary",
            |s| s.secondary_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 80.0 } else { 40.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 7.0))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.secondary_container(),
                    ColorSpecDelegateImpl2021.secondary(),
                    10.0,
                    TonePolarity::Nearer,
                    false,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_secondary(&self) -> DynamicColor {
        let secondary = self.secondary();
        DynamicColor::from_palette(
            "on_secondary",
            |s| s.secondary_palette.clone(),
            Some(|s: &DynamicScheme| {
                if is_monochrome(s) {
                    if s.is_dark {
                        10.0
                    } else {
                        100.0
                    }
                } else if s.is_dark {
                    20.0
                } else {
                    100.0
                }
            }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(secondary.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn secondary_container(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "secondary_container",
            |s| s.secondary_palette.clone(),
            Some(|s: &DynamicScheme| {
                let initial_tone = if s.is_dark { 30.0 } else { 90.0 };
                if is_monochrome(s) {
                    if s.is_dark {
                        30.0
                    } else {
                        85.0
                    }
                } else if !is_fidelity(s) {
                    initial_tone
                } else {
                    find_desired_chroma_by_tone(
                        s.secondary_palette.hue(),
                        s.secondary_palette.chroma(),
                        initial_tone,
                        !s.is_dark,
                    )
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.secondary_container(),
                    ColorSpecDelegateImpl2021.secondary(),
                    10.0,
                    TonePolarity::Nearer,
                    false,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_secondary_container(&self) -> DynamicColor {
        let secondary_container = self.secondary_container();
        let secondary_container_for_tone = secondary_container.clone();
        DynamicColor::from_palette(
            "on_secondary_container",
            |s| s.secondary_palette.clone(),
            Some(move |s: &DynamicScheme| {
                if is_monochrome(s) {
                    if s.is_dark {
                        90.0
                    } else {
                        10.0
                    }
                } else if !is_fidelity(s) {
                    if s.is_dark {
                        90.0
                    } else {
                        30.0
                    }
                } else {
                    DynamicColor::foreground_tone(
                        (secondary_container_for_tone.tone)(s),
                        4.5,
                    )
                }
            }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(secondary_container.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    // ================================================================
    // Tertiaries [T]
    // ================================================================

    fn tertiary(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "tertiary",
            |s| s.tertiary_palette.clone(),
            Some(|s: &DynamicScheme| {
                if is_monochrome(s) {
                    if s.is_dark {
                        90.0
                    } else {
                        25.0
                    }
                } else if s.is_dark {
                    80.0
                } else {
                    40.0
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 7.0))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.tertiary_container(),
                    ColorSpecDelegateImpl2021.tertiary(),
                    10.0,
                    TonePolarity::Nearer,
                    false,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_tertiary(&self) -> DynamicColor {
        let tertiary = self.tertiary();
        DynamicColor::from_palette(
            "on_tertiary",
            |s| s.tertiary_palette.clone(),
            Some(|s: &DynamicScheme| {
                if is_monochrome(s) {
                    if s.is_dark {
                        10.0
                    } else {
                        90.0
                    }
                } else if s.is_dark {
                    20.0
                } else {
                    100.0
                }
            }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(tertiary.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn tertiary_container(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "tertiary_container",
            |s| s.tertiary_palette.clone(),
            Some(|s: &DynamicScheme| {
                if is_monochrome(s) {
                    if s.is_dark {
                        60.0
                    } else {
                        49.0
                    }
                } else if !is_fidelity(s) {
                    if s.is_dark {
                        30.0
                    } else {
                        90.0
                    }
                } else {
                    let proposed_hct = s
                        .tertiary_palette
                        .get_hct(s.source_color_hct.tone());
                    DislikeAnalyzer::fix_if_disliked(&proposed_hct).tone()
                }
            }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.tertiary_container(),
                    ColorSpecDelegateImpl2021.tertiary(),
                    10.0,
                    TonePolarity::Nearer,
                    false,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_tertiary_container(&self) -> DynamicColor {
        let tertiary_container = self.tertiary_container();
        let tertiary_container_for_tone = tertiary_container.clone();
        DynamicColor::from_palette(
            "on_tertiary_container",
            |s| s.tertiary_palette.clone(),
            Some(move |s: &DynamicScheme| {
                if is_monochrome(s) {
                    if s.is_dark {
                        0.0
                    } else {
                        100.0
                    }
                } else if !is_fidelity(s) {
                    if s.is_dark {
                        90.0
                    } else {
                        30.0
                    }
                } else {
                    DynamicColor::foreground_tone(
                        (tertiary_container_for_tone.tone)(s),
                        4.5,
                    )
                }
            }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(tertiary_container.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    // ================================================================
    // Errors [E]
    // ================================================================

    fn error(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "error",
            |s| s.error_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 80.0 } else { 40.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 7.0))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.error_container(),
                    ColorSpecDelegateImpl2021.error(),
                    10.0,
                    TonePolarity::Nearer,
                    false,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_error(&self) -> DynamicColor {
        let error = self.error();
        DynamicColor::from_palette(
            "on_error",
            |s| s.error_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 20.0 } else { 100.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(error.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn error_container(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "error_container",
            |s| s.error_palette.clone(),
            Some(|s: &DynamicScheme| if s.is_dark { 30.0 } else { 90.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.error_container(),
                    ColorSpecDelegateImpl2021.error(),
                    10.0,
                    TonePolarity::Nearer,
                    false,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_error_container(&self) -> DynamicColor {
        let error_container = self.error_container();
        DynamicColor::from_palette(
            "on_error_container",
            |s| s.error_palette.clone(),
            Some(|s: &DynamicScheme| {
                if is_monochrome(s) {
                    if s.is_dark {
                        90.0
                    } else {
                        10.0
                    }
                } else if s.is_dark {
                    90.0
                } else {
                    30.0
                }
            }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(error_container.clone())),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    // ================================================================
    // Primary Fixed Colors [PF]
    // ================================================================

    fn primary_fixed(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "primary_fixed",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 40.0 } else { 90.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.primary_fixed(),
                    ColorSpecDelegateImpl2021.primary_fixed_dim(),
                    10.0,
                    TonePolarity::Lighter,
                    true,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn primary_fixed_dim(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "primary_fixed_dim",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 30.0 } else { 80.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.primary_fixed(),
                    ColorSpecDelegateImpl2021.primary_fixed_dim(),
                    10.0,
                    TonePolarity::Lighter,
                    true,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_primary_fixed(&self) -> DynamicColor {
        let primary_fixed_dim = self.primary_fixed_dim();
        let primary_fixed = self.primary_fixed();
        DynamicColor::from_palette(
            "on_primary_fixed",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 100.0 } else { 10.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(primary_fixed_dim.clone())),
            Some(move |_s: &DynamicScheme| Some(primary_fixed.clone())),
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn on_primary_fixed_variant(&self) -> DynamicColor {
        let primary_fixed_dim = self.primary_fixed_dim();
        let primary_fixed = self.primary_fixed();
        DynamicColor::from_palette(
            "on_primary_fixed_variant",
            |s| s.primary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 90.0 } else { 30.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(primary_fixed_dim.clone())),
            Some(move |_s: &DynamicScheme| Some(primary_fixed.clone())),
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    // ================================================================
    // Secondary Fixed Colors [QF]
    // ================================================================

    fn secondary_fixed(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "secondary_fixed",
            |s| s.secondary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 80.0 } else { 90.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.secondary_fixed(),
                    ColorSpecDelegateImpl2021.secondary_fixed_dim(),
                    10.0,
                    TonePolarity::Lighter,
                    true,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn secondary_fixed_dim(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "secondary_fixed_dim",
            |s| s.secondary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 70.0 } else { 80.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.secondary_fixed(),
                    ColorSpecDelegateImpl2021.secondary_fixed_dim(),
                    10.0,
                    TonePolarity::Lighter,
                    true,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_secondary_fixed(&self) -> DynamicColor {
        let secondary_fixed_dim = self.secondary_fixed_dim();
        let secondary_fixed = self.secondary_fixed();
        DynamicColor::from_palette(
            "on_secondary_fixed",
            |s| s.secondary_palette.clone(),
            Some(|_s: &DynamicScheme| 10.0),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(secondary_fixed_dim.clone())),
            Some(move |_s: &DynamicScheme| Some(secondary_fixed.clone())),
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn on_secondary_fixed_variant(&self) -> DynamicColor {
        let secondary_fixed_dim = self.secondary_fixed_dim();
        let secondary_fixed = self.secondary_fixed();
        DynamicColor::from_palette(
            "on_secondary_fixed_variant",
            |s| s.secondary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 25.0 } else { 30.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(secondary_fixed_dim.clone())),
            Some(move |_s: &DynamicScheme| Some(secondary_fixed.clone())),
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    // ================================================================
    // Tertiary Fixed Colors [TF]
    // ================================================================

    fn tertiary_fixed(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "tertiary_fixed",
            |s| s.tertiary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 40.0 } else { 90.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.tertiary_fixed(),
                    ColorSpecDelegateImpl2021.tertiary_fixed_dim(),
                    10.0,
                    TonePolarity::Lighter,
                    true,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn tertiary_fixed_dim(&self) -> DynamicColor {
        let delegate = ColorSpecDelegateImpl2021;
        DynamicColor::from_palette(
            "tertiary_fixed_dim",
            |s| s.tertiary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 30.0 } else { 80.0 }),
            true,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |s: &DynamicScheme| Some(delegate.highest_surface(s))),
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(1.0, 1.0, 3.0, 4.5))),
            Some(|_s: &DynamicScheme| {
                Some(ToneDeltaPair::new(
                    ColorSpecDelegateImpl2021.tertiary_fixed(),
                    ColorSpecDelegateImpl2021.tertiary_fixed_dim(),
                    10.0,
                    TonePolarity::Lighter,
                    true,
                    DeltaConstraint::Exact,
                ))
            }),
        )
    }

    fn on_tertiary_fixed(&self) -> DynamicColor {
        let tertiary_fixed_dim = self.tertiary_fixed_dim();
        let tertiary_fixed = self.tertiary_fixed();
        DynamicColor::from_palette(
            "on_tertiary_fixed",
            |s| s.tertiary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 100.0 } else { 10.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(tertiary_fixed_dim.clone())),
            Some(move |_s: &DynamicScheme| Some(tertiary_fixed.clone())),
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(4.5, 7.0, 11.0, 21.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }

    fn on_tertiary_fixed_variant(&self) -> DynamicColor {
        let tertiary_fixed_dim = self.tertiary_fixed_dim();
        let tertiary_fixed = self.tertiary_fixed();
        DynamicColor::from_palette(
            "on_tertiary_fixed_variant",
            |s| s.tertiary_palette.clone(),
            Some(|s: &DynamicScheme| if is_monochrome(s) { 90.0 } else { 30.0 }),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            Some(move |_s: &DynamicScheme| Some(tertiary_fixed_dim.clone())),
            Some(move |_s: &DynamicScheme| Some(tertiary_fixed.clone())),
            Some(|_s: &DynamicScheme| Some(ContrastCurve::new(3.0, 4.5, 7.0, 11.0))),
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::color::dynamiccolor::{DynamicSchemeOptions, SpecVersion, Variant};
    use crate::material::color::hct::Hct;

    fn new_2021_scheme(mut options: DynamicSchemeOptions) -> DynamicScheme {
        options.spec_version = Some(SpecVersion::Spec2021);
        DynamicScheme::new(options)
    }

    #[test]
    fn test_delegate_creation() {
        let delegate = ColorSpecDelegateImpl2021::new();
        let _background = delegate.background();
    }

    #[test]
    fn test_is_fidelity() {
        let hct = Hct::from_int(0xFF0000FF);

        let fidelity_scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::Fidelity,
            0.0,
            false,
        ));
        assert!(is_fidelity(&fidelity_scheme));

        let content_scheme =
            new_2021_scheme(DynamicSchemeOptions::new(hct, Variant::Content, 0.0, false));
        assert!(is_fidelity(&content_scheme));

        let tonal_spot_scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::TonalSpot,
            0.0,
            false,
        ));
        assert!(!is_fidelity(&tonal_spot_scheme));
    }

    #[test]
    fn test_is_monochrome() {
        let hct = Hct::from_int(0xFF0000FF);

        let mono_scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::Monochrome,
            0.0,
            false,
        ));
        assert!(is_monochrome(&mono_scheme));

        let vibrant_scheme =
            new_2021_scheme(DynamicSchemeOptions::new(hct, Variant::Vibrant, 0.0, false));
        assert!(!is_monochrome(&vibrant_scheme));
    }

    #[test]
    fn test_background_light_mode() {
        let delegate = ColorSpecDelegateImpl2021::new();
        let hct = Hct::from_int(0xFF0000FF);
        let scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::TonalSpot,
            0.0,
            false, // light mode
        ));

        let background = delegate.background();
        let tone = background.get_tone(&scheme);
        assert_eq!(tone, 98.0);
    }

    #[test]
    fn test_background_dark_mode() {
        let delegate = ColorSpecDelegateImpl2021::new();
        let hct = Hct::from_int(0xFF0000FF);
        let scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::TonalSpot,
            0.0,
            true, // dark mode
        ));

        let background = delegate.background();
        let tone = background.get_tone(&scheme);
        assert_eq!(tone, 6.0);
    }

    #[test]
    fn test_primary_monochrome_dark() {
        let delegate = ColorSpecDelegateImpl2021::new();
        let hct = Hct::from_int(0xFF0000FF);
        let scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::Monochrome,
            0.0,
            true,
        ));

        let primary = delegate.primary();
        let tone = primary.get_tone(&scheme);
        assert_eq!(tone, 100.0);
        assert_eq!(primary.get_argb(&scheme), 0xFFFFFFFF);
    }

    #[test]
    fn test_primary_monochrome_light() {
        let delegate = ColorSpecDelegateImpl2021::new();
        let hct = Hct::from_int(0xFF0000FF);
        let scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::Monochrome,
            0.0,
            false,
        ));

        let primary = delegate.primary();
        let tone = primary.get_tone(&scheme);
        assert_eq!(tone, 0.0);
        assert_eq!(primary.get_argb(&scheme), 0xFF000000);
    }

    #[test]
    fn test_primary_tonal_spot_light() {
        let delegate = ColorSpecDelegateImpl2021::new();
        let hct = Hct::from_int(0xFF0000FF);
        let scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::TonalSpot,
            0.0,
            false,
        ));

        let primary = delegate.primary();
        let tone = primary.get_tone(&scheme);
        assert_eq!(tone, 40.0);
    }

    #[test]
    fn test_primary_tonal_spot_dark() {
        let delegate = ColorSpecDelegateImpl2021::new();
        let hct = Hct::from_int(0xFF0000FF);
        let scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::TonalSpot,
            0.0,
            true,
        ));

        let primary = delegate.primary();
        let tone = primary.get_tone(&scheme);
        assert_eq!(tone, 80.0);
    }

    #[test]
    fn test_all_surface_colors_exist() {
        let delegate = ColorSpecDelegateImpl2021::new();

        let _background = delegate.background();
        let _surface = delegate.surface();
        let _surface_dim = delegate.surface_dim();
        let _surface_bright = delegate.surface_bright();
        let _surface_container_lowest = delegate.surface_container_lowest();
        let _surface_container_low = delegate.surface_container_low();
        let _surface_container = delegate.surface_container();
        let _surface_container_high = delegate.surface_container_high();
        let _surface_container_highest = delegate.surface_container_highest();
    }

    #[test]
    fn test_all_primary_colors_exist() {
        let delegate = ColorSpecDelegateImpl2021::new();

        let _primary = delegate.primary();
        let _on_primary = delegate.on_primary();
        let _primary_container = delegate.primary_container();
        let _on_primary_container = delegate.on_primary_container();
        let _inverse_primary = delegate.inverse_primary();
        let _primary_fixed = delegate.primary_fixed();
        let _primary_fixed_dim = delegate.primary_fixed_dim();
        let _on_primary_fixed = delegate.on_primary_fixed();
        let _on_primary_fixed_variant = delegate.on_primary_fixed_variant();
    }

    #[test]
    fn test_error_colors_exist() {
        let delegate = ColorSpecDelegateImpl2021::new();

        let _error = delegate.error();
        let _on_error = delegate.on_error();
        let _error_container = delegate.error_container();
        let _on_error_container = delegate.on_error_container();
    }

    #[test]
    fn test_shadow_and_scrim_are_black() {
        let delegate = ColorSpecDelegateImpl2021::new();
        let hct = Hct::from_int(0xFF0000FF);
        let scheme = new_2021_scheme(DynamicSchemeOptions::new(
            hct,
            Variant::TonalSpot,
            0.0,
            false,
        ));

        let shadow = delegate.shadow();
        let scrim = delegate.scrim();

        assert_eq!(shadow.get_tone(&scheme), 0.0);
        assert_eq!(scrim.get_tone(&scheme), 0.0);
    }
}
