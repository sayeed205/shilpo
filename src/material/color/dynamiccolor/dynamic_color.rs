
use crate::material::color::contrast::Contrast;
use crate::material::color::hct::Hct;
use crate::material::color::palettes::TonalPalette;
use std::sync::Arc;

use super::color_calculation::get_spec;
use super::{ContrastCurve, ToneDeltaPair};

/// Errors that can occur when creating a DynamicColor
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicColorError {
    /// second_background was provided without background
    MissingBackground {
        /// Name of the invalid dynamic color definition.
        color_name: String,
    },
    /// contrast_curve was provided without background
    MissingBackgroundForContrast {
        /// Name of the invalid dynamic color definition.
        color_name: String,
    },
    /// background was provided without contrast_curve
    MissingContrastCurve {
        /// Name of the invalid dynamic color definition.
        color_name: String,
    },
}

impl std::fmt::Display for DynamicColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBackground { color_name } => {
                write!(
                    f,
                    "Color '{}' has second_background but no background",
                    color_name
                )
            }
            Self::MissingBackgroundForContrast { color_name } => {
                write!(
                    f,
                    "Color '{}' has contrast_curve but no background",
                    color_name
                )
            }
            Self::MissingContrastCurve { color_name } => {
                write!(
                    f,
                    "Color '{}' has background but no contrast_curve",
                    color_name
                )
            }
        }
    }
}

impl std::error::Error for DynamicColorError {}

// Import DynamicScheme from dynamic_scheme module
pub use super::dynamic_scheme::DynamicScheme;

/// A color that adjusts itself based on UI state provided by DynamicScheme.
///
/// Colors without backgrounds do not change tone when contrast changes. Colors
/// with backgrounds become closer to their background as contrast lowers, and
/// further when contrast increases.
///
/// # Example
///
/// ```ignore
/// use mcu_dynamiccolor::DynamicColor;
/// use mcu_palettes::TonalPalette;
///
/// let color = DynamicColor::from_palette(
///     "primary",
///     |scheme| scheme.primary_palette.clone(),
///     Some(|_scheme| 40.0),
///     false,
///     None,
///     None,
///     None,
///     None,
///     None,
/// );
/// ```
#[derive(Clone)]
pub struct DynamicColor {
    /// The name of the dynamic color
    pub name: String,

    /// Function that provides a TonalPalette given DynamicScheme
    pub palette: Arc<dyn Fn(&DynamicScheme) -> TonalPalette + Send + Sync>,

    /// Function that provides a tone given DynamicScheme
    pub tone: Arc<dyn Fn(&DynamicScheme) -> f64 + Send + Sync>,

    /// Whether this dynamic color is a background
    pub is_background: bool,

    /// A factor that multiplies the chroma for this color
    pub chroma_multiplier: Option<Arc<dyn Fn(&DynamicScheme) -> f64 + Send + Sync>>,

    /// The background of the dynamic color, if it exists
    pub background: Option<Arc<dyn Fn(&DynamicScheme) -> Option<DynamicColor> + Send + Sync>>,

    /// A second background of the dynamic color, if it exists
    pub second_background:
        Option<Arc<dyn Fn(&DynamicScheme) -> Option<DynamicColor> + Send + Sync>>,

    /// A ContrastCurve object specifying how its contrast against its background should behave
    pub contrast_curve: Option<Arc<dyn Fn(&DynamicScheme) -> Option<ContrastCurve> + Send + Sync>>,

    /// A ToneDeltaPair object specifying a tone delta constraint between two colors
    pub tone_delta_pair: Option<Arc<dyn Fn(&DynamicScheme) -> Option<ToneDeltaPair> + Send + Sync>>,
}

impl DynamicColor {
    /// Create a DynamicColor defined by a TonalPalette and HCT tone.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the dynamic color
    /// * `palette` - Function that provides a TonalPalette given DynamicScheme
    /// * `tone` - Function that provides a tone given DynamicScheme (if None, defaults to background tone or 50)
    /// * `is_background` - Whether this dynamic color is a background
    /// * `chroma_multiplier` - Optional function that provides a chroma multiplier
    /// * `background` - Optional function that provides the background DynamicColor
    /// * `second_background` - Optional function that provides a second background
    /// * `contrast_curve` - Optional function that provides a ContrastCurve
    /// * `tone_delta_pair` - Optional function that provides a ToneDeltaPair
    ///
    /// # Panics
    ///
    /// Panics if the configuration is invalid. Use [`try_from_palette`](Self::try_from_palette)
    /// for fallible construction.
    #[allow(clippy::too_many_arguments)]
    pub fn from_palette(
        name: impl Into<String>,
        palette: impl Fn(&DynamicScheme) -> TonalPalette + Send + Sync + 'static,
        tone: Option<impl Fn(&DynamicScheme) -> f64 + Send + Sync + 'static>,
        is_background: bool,
        chroma_multiplier: Option<impl Fn(&DynamicScheme) -> f64 + Send + Sync + 'static>,
        background: Option<impl Fn(&DynamicScheme) -> Option<DynamicColor> + Send + Sync + 'static>,
        second_background: Option<
            impl Fn(&DynamicScheme) -> Option<DynamicColor> + Send + Sync + 'static,
        >,
        contrast_curve: Option<
            impl Fn(&DynamicScheme) -> Option<ContrastCurve> + Send + Sync + 'static,
        >,
        tone_delta_pair: Option<
            impl Fn(&DynamicScheme) -> Option<ToneDeltaPair> + Send + Sync + 'static,
        >,
    ) -> Self {
        Self::try_from_palette(
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
        .expect("invalid DynamicColor configuration")
    }

    /// Try to create a DynamicColor defined by a TonalPalette and HCT tone.
    ///
    /// This is the fallible version of [`from_palette`](Self::from_palette).
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the dynamic color
    /// * `palette` - Function that provides a TonalPalette given DynamicScheme
    /// * `tone` - Function that provides a tone given DynamicScheme (if None, defaults to 50)
    /// * `is_background` - Whether this dynamic color is a background
    /// * `chroma_multiplier` - Optional function that provides a chroma multiplier
    /// * `background` - Optional function that provides the background DynamicColor
    /// * `second_background` - Optional function that provides a second background
    /// * `contrast_curve` - Optional function that provides a ContrastCurve
    /// * `tone_delta_pair` - Optional function that provides a ToneDeltaPair
    ///
    /// # Errors
    ///
    /// Returns [`DynamicColorError`] if:
    /// - `second_background` is provided without `background`
    /// - `contrast_curve` is provided without `background`
    /// - `background` is provided without `contrast_curve`
    #[allow(clippy::too_many_arguments)]
    pub fn try_from_palette(
        name: impl Into<String>,
        palette: impl Fn(&DynamicScheme) -> TonalPalette + Send + Sync + 'static,
        tone: Option<impl Fn(&DynamicScheme) -> f64 + Send + Sync + 'static>,
        is_background: bool,
        chroma_multiplier: Option<impl Fn(&DynamicScheme) -> f64 + Send + Sync + 'static>,
        background: Option<impl Fn(&DynamicScheme) -> Option<DynamicColor> + Send + Sync + 'static>,
        second_background: Option<
            impl Fn(&DynamicScheme) -> Option<DynamicColor> + Send + Sync + 'static,
        >,
        contrast_curve: Option<
            impl Fn(&DynamicScheme) -> Option<ContrastCurve> + Send + Sync + 'static,
        >,
        tone_delta_pair: Option<
            impl Fn(&DynamicScheme) -> Option<ToneDeltaPair> + Send + Sync + 'static,
        >,
    ) -> Result<Self, DynamicColorError> {
        let name = name.into();

        // Validate constraints
        if background.is_none() && second_background.is_some() {
            return Err(DynamicColorError::MissingBackground { color_name: name });
        }
        if background.is_none() && contrast_curve.is_some() {
            return Err(DynamicColorError::MissingBackgroundForContrast { color_name: name });
        }
        if background.is_some() && contrast_curve.is_none() {
            return Err(DynamicColorError::MissingContrastCurve { color_name: name });
        }

        // The canonical default tone is the background's resolved tone (or
        // 50 when no background is supplied), not a fixed 50 for every role.
        let background: Option<
            Arc<dyn Fn(&DynamicScheme) -> Option<DynamicColor> + Send + Sync>,
        > = background.map(|f| Arc::new(f) as _);
        let tone_fn: Arc<dyn Fn(&DynamicScheme) -> f64 + Send + Sync> = match tone {
            Some(t) => Arc::new(t),
            None => match background.clone() {
                Some(background) => Arc::new(move |scheme: &DynamicScheme| {
                    background(scheme)
                        .map(|color| color.get_tone(scheme))
                        .unwrap_or(50.0)
                }),
                None => Arc::new(|_scheme: &DynamicScheme| 50.0),
            },
        };

        Ok(DynamicColor {
            name,
            palette: Arc::new(palette),
            tone: tone_fn,
            is_background,
            chroma_multiplier: chroma_multiplier
                .map(|f| Arc::new(f) as Arc<dyn Fn(&DynamicScheme) -> f64 + Send + Sync>),
            background,
            second_background: second_background.map(|f| {
                Arc::new(f) as Arc<dyn Fn(&DynamicScheme) -> Option<DynamicColor> + Send + Sync>
            }),
            contrast_curve: contrast_curve.map(|f| {
                Arc::new(f) as Arc<dyn Fn(&DynamicScheme) -> Option<ContrastCurve> + Send + Sync>
            }),
            tone_delta_pair: tone_delta_pair.map(|f| {
                Arc::new(f) as Arc<dyn Fn(&DynamicScheme) -> Option<ToneDeltaPair> + Send + Sync>
            }),
        })
    }

    /// Returns an ARGB integer (i.e., a hex code).
    ///
    /// # Arguments
    ///
    /// * `scheme` - Defines the conditions of the user interface
    ///
    /// # Returns
    ///
    /// ARGB color as a 32-bit integer
    pub fn get_argb(&self, scheme: &DynamicScheme) -> u32 {
        self.get_hct(scheme).to_int()
    }

    /// Returns a color, expressed in the HCT color space.
    ///
    /// Delegates to the spec-versioned ColorCalculationDelegate for the actual
    /// calculation. The delegate handles contrast adjustments, tone delta pairs,
    /// and spec-specific behavior (e.g., 2025 spec chroma multiplier).
    ///
    /// # Arguments
    ///
    /// * `scheme` - Defines the conditions of the user interface
    ///
    /// # Returns
    ///
    /// HCT color
    pub fn get_hct(&self, scheme: &DynamicScheme) -> Hct {
        // Delegate to spec-versioned implementation
        get_spec(scheme.spec_version).get_hct(scheme, self)
    }

    /// Returns a tone, T in the HCT color space.
    ///
    /// Delegates to the spec-versioned ColorCalculationDelegate for the actual
    /// calculation. The delegate handles contrast adjustments, tone delta pairs,
    /// awkward zone handling, and spec-specific behavior.
    ///
    /// # Arguments
    ///
    /// * `scheme` - Defines the conditions of the user interface
    ///
    /// # Returns
    ///
    /// Tone value (0.0 to 100.0)
    pub fn get_tone(&self, scheme: &DynamicScheme) -> f64 {
        // Delegate to spec-versioned implementation
        get_spec(scheme.spec_version).get_tone(scheme, self)
    }

    /// Given a background tone, finds a foreground tone that achieves
    /// the target contrast ratio as closely as possible.
    ///
    /// # Arguments
    ///
    /// * `bg_tone` - The tone of the background (0.0 to 100.0)
    /// * `ratio` - The target contrast ratio (e.g., 4.5 for WCAG AA)
    ///
    /// # Returns
    ///
    /// A tone value that achieves the target contrast ratio as closely as possible
    pub fn foreground_tone(bg_tone: f64, ratio: f64) -> f64 {
        let lighter_tone = Contrast::lighter_clamped(bg_tone, ratio);
        let darker_tone = Contrast::darker_clamped(bg_tone, ratio);
        let lighter_ratio = Contrast::ratio_of_tones(lighter_tone, bg_tone);
        let darker_ratio = Contrast::ratio_of_tones(darker_tone, bg_tone);
        let prefers_lighter = Self::tone_prefers_light_foreground(bg_tone);

        if prefers_lighter {
            // Handle edge case where both fail to reach ratio
            let negligible_diff = (lighter_ratio - darker_ratio).abs() < 0.1
                && lighter_ratio < ratio
                && darker_ratio < ratio;
            if lighter_ratio >= ratio || lighter_ratio >= darker_ratio || negligible_diff {
                lighter_tone
            } else {
                darker_tone
            }
        } else {
            if darker_ratio >= ratio || darker_ratio >= lighter_ratio {
                darker_tone
            } else {
                lighter_tone
            }
        }
    }

    /// Returns whether the tone prefers a light foreground.
    ///
    /// People prefer white foregrounds on ~T60-70. Humans turn out to prefer
    /// having a dark background with light foreground on tones below 60.
    ///
    /// # Arguments
    ///
    /// * `tone` - The tone to check (0.0 to 100.0)
    ///
    /// # Returns
    ///
    /// True if the tone prefers a light foreground, false otherwise
    pub fn tone_prefers_light_foreground(tone: f64) -> bool {
        tone.round() < 60.0
    }

    /// Returns whether the tone can reach 4.5:1 contrast with a lighter color.
    ///
    /// # Arguments
    ///
    /// * `tone` - The tone to check (0.0 to 100.0)
    ///
    /// # Returns
    ///
    /// True if the tone can achieve 4.5:1 contrast with white, false otherwise
    pub fn tone_allows_light_foreground(tone: f64) -> bool {
        tone.round() <= 49.0
    }

    /// Adjusts a tone such that white has 4.5 contrast, if the tone
    /// is reasonably close to supporting it.
    ///
    /// # Arguments
    ///
    /// * `tone` - The tone to potentially adjust (0.0 to 100.0)
    ///
    /// # Returns
    ///
    /// The adjusted tone, or the original tone if no adjustment is needed
    pub fn enable_light_foreground(tone: f64) -> f64 {
        if Self::tone_prefers_light_foreground(tone) && !Self::tone_allows_light_foreground(tone) {
            49.0
        } else {
            tone
        }
    }
}

// Note: Debug implementation omitted for function pointers
// Clone is already derived above

#[cfg(test)]
mod tests {
    use super::*;

    // Helper functions for tests
    fn test_palette(_scheme: &DynamicScheme) -> TonalPalette {
        TonalPalette::from_hue_and_chroma(200.0, 50.0)
    }

    fn test_tone(_scheme: &DynamicScheme) -> f64 {
        40.0
    }

    fn surface_tone(_scheme: &DynamicScheme) -> f64 {
        95.0
    }

    // Basic tests for DynamicColor structure
    // More complex tests with actual resolution will be added when
    // the full contrast calculation logic is implemented

    #[test]
    fn test_dynamic_color_can_be_created() {
        let color = DynamicColor::from_palette(
            "test_color",
            test_palette,
            Some(test_tone as fn(&DynamicScheme) -> f64),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        );

        assert_eq!(color.name, "test_color");
        assert!(!color.is_background);
    }

    #[test]
    fn test_dynamic_color_is_background() {
        let color = DynamicColor::from_palette(
            "surface",
            test_palette,
            Some(surface_tone as fn(&DynamicScheme) -> f64),
            true, // is_background
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        );

        assert!(color.is_background);
    }

    #[test]
    fn test_dynamic_color_clone() {
        let color1 = DynamicColor::from_palette(
            "test",
            test_palette,
            Some(test_tone as fn(&DynamicScheme) -> f64),
            false,
            None::<fn(&DynamicScheme) -> f64>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<DynamicColor>>,
            None::<fn(&DynamicScheme) -> Option<ContrastCurve>>,
            None::<fn(&DynamicScheme) -> Option<ToneDeltaPair>>,
        );

        let color2 = color1.clone();
        assert_eq!(color1.name, color2.name);
        assert_eq!(color1.is_background, color2.is_background);
    }
}
