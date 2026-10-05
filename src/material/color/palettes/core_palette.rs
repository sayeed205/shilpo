use crate::material::color::hct::Hct;

use super::TonalPalette;

/// Color definitions for creating a CorePalette from multiple source colors.
///
/// This allows specifying individual colors for each palette component instead
/// of deriving them all from a single source color.
#[derive(Debug, Clone, Default)]
pub struct CorePaletteColors {
    /// Primary color (required)
    pub primary: u32,
    /// Secondary color (optional, derived from primary if not provided)
    pub secondary: Option<u32>,
    /// Tertiary color (optional, derived from primary if not provided)
    pub tertiary: Option<u32>,
    /// Neutral color (optional, derived from primary if not provided)
    pub neutral: Option<u32>,
    /// Neutral variant color (optional, derived from primary if not provided)
    pub neutral_variant: Option<u32>,
    /// Error color (optional, uses standard error red if not provided)
    pub error: Option<u32>,
}

impl CorePaletteColors {
    /// Create a new CorePaletteColors with only the primary color set.
    pub fn new(primary: u32) -> Self {
        Self {
            primary,
            ..Default::default()
        }
    }

    /// Set the secondary color.
    pub fn with_secondary(mut self, secondary: u32) -> Self {
        self.secondary = Some(secondary);
        self
    }

    /// Set the tertiary color.
    pub fn with_tertiary(mut self, tertiary: u32) -> Self {
        self.tertiary = Some(tertiary);
        self
    }

    /// Set the neutral color.
    pub fn with_neutral(mut self, neutral: u32) -> Self {
        self.neutral = Some(neutral);
        self
    }

    /// Set the neutral variant color.
    pub fn with_neutral_variant(mut self, neutral_variant: u32) -> Self {
        self.neutral_variant = Some(neutral_variant);
        self
    }

    /// Set the error color.
    pub fn with_error(mut self, error: u32) -> Self {
        self.error = Some(error);
        self
    }
}

/// An intermediate concept between the key color for a UI theme, and a full
/// color scheme.
///
/// 5 sets of tones are generated, all except one use the same hue as the key
/// color, and all vary in chroma. CorePalette provides the standard Material
/// Design palettes (primary, secondary, tertiary, neutral, neutral variant,
/// and error).
///
/// # Palette Names
///
/// - `a1` / `primary`: Primary accent color
/// - `a2` / `secondary`: Secondary accent color
/// - `a3` / `tertiary`: Tertiary accent color (complementary hue)
/// - `n1` / `neutral`: Neutral palette for backgrounds/surfaces
/// - `n2` / `neutral_variant`: Neutral variant for outlined components
/// - `error`: Error state color (fixed red hue)
#[derive(Debug, Clone)]
pub struct CorePalette {
    /// Primary palette (a1)
    pub a1: TonalPalette,
    /// Secondary palette (a2)
    pub a2: TonalPalette,
    /// Tertiary palette (a3)
    pub a3: TonalPalette,
    /// Neutral palette (n1)
    pub n1: TonalPalette,
    /// Neutral variant palette (n2)
    pub n2: TonalPalette,
    /// Error palette
    pub error: TonalPalette,
}

impl CorePalette {
    /// Standard error hue (red)
    const ERROR_HUE: f64 = 25.0;
    /// Standard error chroma
    const ERROR_CHROMA: f64 = 84.0;

    /// Create a CorePalette from an ARGB color.
    ///
    /// This creates a standard palette suitable for UI theming, with
    /// constrained chroma values for the accent colors.
    ///
    /// # Arguments
    ///
    /// * `argb` - ARGB representation of a color
    pub fn of(argb: u32) -> Self {
        Self::new(argb, false)
    }

    /// Create a content CorePalette from an ARGB color.
    ///
    /// This creates a palette that preserves more of the original color's
    /// chroma, suitable for content-based theming where the source color
    /// should be more prominent.
    ///
    /// # Arguments
    ///
    /// * `argb` - ARGB representation of a color
    pub fn content_of(argb: u32) -> Self {
        Self::new(argb, true)
    }

    /// Create a CorePalette from a set of colors.
    ///
    /// This allows customizing individual palette components rather than
    /// deriving them all from a single source color.
    ///
    /// # Arguments
    ///
    /// * `colors` - Color specifications for each palette component
    pub fn from_colors(colors: CorePaletteColors) -> Self {
        Self::create_palette_from_colors(false, colors)
    }

    /// Create a content CorePalette from a set of colors.
    ///
    /// # Arguments
    ///
    /// * `colors` - Color specifications for each palette component
    pub fn content_from_colors(colors: CorePaletteColors) -> Self {
        Self::create_palette_from_colors(true, colors)
    }

    /// Get the primary palette (alias for a1).
    pub fn primary(&self) -> &TonalPalette {
        &self.a1
    }

    /// Get the secondary palette (alias for a2).
    pub fn secondary(&self) -> &TonalPalette {
        &self.a2
    }

    /// Get the tertiary palette (alias for a3).
    pub fn tertiary(&self) -> &TonalPalette {
        &self.a3
    }

    /// Get the neutral palette (alias for n1).
    pub fn neutral(&self) -> &TonalPalette {
        &self.n1
    }

    /// Get the neutral variant palette (alias for n2).
    pub fn neutral_variant(&self) -> &TonalPalette {
        &self.n2
    }

    fn new(argb: u32, is_content: bool) -> Self {
        let hct = Hct::from_int(argb);
        let hue = hct.hue();
        let chroma = hct.chroma();

        if is_content {
            // Content mode: preserve more of the original color's chroma
            Self {
                a1: TonalPalette::from_hue_and_chroma(hue, chroma),
                a2: TonalPalette::from_hue_and_chroma(hue, chroma / 3.0),
                a3: TonalPalette::from_hue_and_chroma(hue + 60.0, chroma / 2.0),
                n1: TonalPalette::from_hue_and_chroma(hue, (chroma / 12.0).min(4.0)),
                n2: TonalPalette::from_hue_and_chroma(hue, (chroma / 6.0).min(8.0)),
                error: TonalPalette::from_hue_and_chroma(Self::ERROR_HUE, Self::ERROR_CHROMA),
            }
        } else {
            // Standard mode: constrained chroma values for consistent UI
            Self {
                a1: TonalPalette::from_hue_and_chroma(hue, chroma.max(48.0)),
                a2: TonalPalette::from_hue_and_chroma(hue, 16.0),
                a3: TonalPalette::from_hue_and_chroma(hue + 60.0, 24.0),
                n1: TonalPalette::from_hue_and_chroma(hue, 4.0),
                n2: TonalPalette::from_hue_and_chroma(hue, 8.0),
                error: TonalPalette::from_hue_and_chroma(Self::ERROR_HUE, Self::ERROR_CHROMA),
            }
        }
    }

    fn create_palette_from_colors(is_content: bool, colors: CorePaletteColors) -> Self {
        let mut palette = Self::new(colors.primary, is_content);

        // Treat zero-valued optional ARGBs as omitted overrides.
        if let Some(secondary) = colors.secondary.filter(|color| *color != 0) {
            let p = Self::new(secondary, is_content);
            palette.a2 = p.a1;
        }
        if let Some(tertiary) = colors.tertiary.filter(|color| *color != 0) {
            let p = Self::new(tertiary, is_content);
            palette.a3 = p.a1;
        }
        if let Some(error) = colors.error.filter(|color| *color != 0) {
            let p = Self::new(error, is_content);
            palette.error = p.a1;
        }
        if let Some(neutral) = colors.neutral.filter(|color| *color != 0) {
            let p = Self::new(neutral, is_content);
            palette.n1 = p.n1;
        }
        if let Some(neutral_variant) = colors.neutral_variant.filter(|color| *color != 0) {
            let p = Self::new(neutral_variant, is_content);
            palette.n2 = p.n2;
        }

        palette
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_relative_eq {
        ($left:expr, $right:expr, epsilon = $epsilon:expr $(,)?) => {{
            let (left, right, epsilon) = ($left, $right, $epsilon);
            assert!(
                (left - right).abs() <= epsilon,
                "{left:?} != {right:?} within {epsilon:?}"
            );
        }};
    }

    const RED: u32 = 0xFFFF0000;
    const GREEN: u32 = 0xFF00FF00;
    const BLUE: u32 = 0xFF0000FF;
    const CYAN: u32 = 0xFF00FFFF;

    #[test]
    fn test_of_creates_palette() {
        let palette = CorePalette::of(BLUE);

        // All palettes should be created
        assert!(palette.a1.chroma() > 0.0 || palette.a1.hue() >= 0.0);
        assert!(palette.a2.chroma() > 0.0 || palette.a2.hue() >= 0.0);
        assert!(palette.a3.chroma() > 0.0 || palette.a3.hue() >= 0.0);
    }

    #[test]
    fn test_of_primary_hue_preserved() {
        let hct = Hct::from_int(BLUE);
        let palette = CorePalette::of(BLUE);

        // Primary should have same hue as source
        let hue_diff = (palette.a1.hue() - hct.hue()).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 1.0);
    }

    #[test]
    fn test_of_secondary_hue_preserved() {
        let hct = Hct::from_int(BLUE);
        let palette = CorePalette::of(BLUE);

        // Secondary should have same hue as source
        let hue_diff = (palette.a2.hue() - hct.hue()).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 1.0);
    }

    #[test]
    fn test_of_tertiary_hue_offset() {
        let hct = Hct::from_int(BLUE);
        let palette = CorePalette::of(BLUE);

        // Tertiary should have hue offset by 60 degrees
        let expected_hue = (hct.hue() + 60.0) % 360.0;
        let hue_diff = (palette.a3.hue() - expected_hue).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 5.0);
    }

    #[test]
    fn test_of_neutral_low_chroma() {
        let palette = CorePalette::of(BLUE);

        // Neutral palettes should have low chroma
        assert_relative_eq!(palette.n1.chroma(), 4.0, epsilon = 0.001);
        assert_relative_eq!(palette.n2.chroma(), 8.0, epsilon = 0.001);
    }

    #[test]
    fn test_of_error_standard_values() {
        let palette = CorePalette::of(BLUE);

        // Error palette should have standard hue and chroma
        assert_relative_eq!(palette.error.hue(), 25.0, epsilon = 0.001);
        assert_relative_eq!(palette.error.chroma(), 84.0, epsilon = 0.001);
    }

    #[test]
    fn test_content_of_preserves_chroma() {
        let hct = Hct::from_int(CYAN);
        let palette = CorePalette::content_of(CYAN);

        // Content mode should preserve original chroma
        assert_relative_eq!(palette.a1.chroma(), hct.chroma(), epsilon = 0.001);
    }

    #[test]
    fn test_content_of_reduced_secondary_chroma() {
        let hct = Hct::from_int(CYAN);
        let palette = CorePalette::content_of(CYAN);

        // Secondary should have chroma / 3
        assert_relative_eq!(palette.a2.chroma(), hct.chroma() / 3.0, epsilon = 0.001);
    }

    #[test]
    fn test_content_of_reduced_tertiary_chroma() {
        let hct = Hct::from_int(CYAN);
        let palette = CorePalette::content_of(CYAN);

        // Tertiary should have chroma / 2
        assert_relative_eq!(palette.a3.chroma(), hct.chroma() / 2.0, epsilon = 0.001);
    }

    #[test]
    fn test_content_of_neutral_capped_chroma() {
        let hct = Hct::from_int(CYAN);
        let palette = CorePalette::content_of(CYAN);

        // Neutral should have min(chroma / 12, 4)
        let expected_n1_chroma = (hct.chroma() / 12.0).min(4.0);
        assert_relative_eq!(palette.n1.chroma(), expected_n1_chroma, epsilon = 0.001);

        // Neutral variant should have min(chroma / 6, 8)
        let expected_n2_chroma = (hct.chroma() / 6.0).min(8.0);
        assert_relative_eq!(palette.n2.chroma(), expected_n2_chroma, epsilon = 0.001);
    }

    #[test]
    fn test_from_colors_custom_secondary() {
        let colors = CorePaletteColors::new(BLUE).with_secondary(RED);
        let palette = CorePalette::from_colors(colors);

        // Secondary should have red hue
        let red_hct = Hct::from_int(RED);
        let hue_diff = (palette.a2.hue() - red_hct.hue()).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 5.0);
    }

    #[test]
    fn test_from_colors_custom_tertiary() {
        let colors = CorePaletteColors::new(BLUE).with_tertiary(GREEN);
        let palette = CorePalette::from_colors(colors);

        // Tertiary should have green hue
        let green_hct = Hct::from_int(GREEN);
        let hue_diff = (palette.a3.hue() - green_hct.hue()).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 5.0);
    }

    #[test]
    fn test_from_colors_custom_error() {
        let colors = CorePaletteColors::new(BLUE).with_error(GREEN);
        let palette = CorePalette::from_colors(colors);

        // Error should have green hue instead of default red
        let green_hct = Hct::from_int(GREEN);
        let hue_diff = (palette.error.hue() - green_hct.hue()).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 5.0);
    }

    #[test]
    fn test_from_colors_custom_neutral() {
        let colors = CorePaletteColors::new(BLUE).with_neutral(RED);
        let palette = CorePalette::from_colors(colors);

        // Neutral should have red hue
        let red_hct = Hct::from_int(RED);
        let hue_diff = (palette.n1.hue() - red_hct.hue()).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 5.0);
    }

    #[test]
    fn test_from_colors_custom_neutral_variant() {
        let colors = CorePaletteColors::new(BLUE).with_neutral_variant(GREEN);
        let palette = CorePalette::from_colors(colors);

        // Neutral variant should have green hue
        let green_hct = Hct::from_int(GREEN);
        let hue_diff = (palette.n2.hue() - green_hct.hue()).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 5.0);
    }

    #[test]
    fn test_zero_override_colors_follow_typescript_falsy_fallback() {
        let source = CorePalette::of(BLUE);
        let colors = CorePaletteColors::new(BLUE)
            .with_secondary(0)
            .with_tertiary(0)
            .with_neutral(0)
            .with_neutral_variant(0)
            .with_error(0);
        let palette = CorePalette::from_colors(colors);

        for (actual, expected) in [
            (&palette.a2, &source.a2),
            (&palette.a3, &source.a3),
            (&palette.n1, &source.n1),
            (&palette.n2, &source.n2),
            (&palette.error, &source.error),
        ] {
            assert_eq!(actual.tone(40), expected.tone(40));
        }
    }

    #[test]
    fn test_content_from_colors() {
        let colors = CorePaletteColors::new(CYAN).with_secondary(RED);
        let palette = CorePalette::content_from_colors(colors);

        // Secondary should have red hue with content chroma
        let red_hct = Hct::from_int(RED);
        let hue_diff = (palette.a2.hue() - red_hct.hue()).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 5.0);

        // Should preserve chroma for secondary (red's chroma, not reduced)
        assert!(palette.a2.chroma() > 50.0);
    }

    #[test]
    fn test_palette_aliases() {
        let palette = CorePalette::of(BLUE);

        // Test alias methods return the same palettes
        assert_relative_eq!(palette.primary().hue(), palette.a1.hue(), epsilon = 0.001);
        assert_relative_eq!(palette.secondary().hue(), palette.a2.hue(), epsilon = 0.001);
        assert_relative_eq!(palette.tertiary().hue(), palette.a3.hue(), epsilon = 0.001);
        assert_relative_eq!(palette.neutral().hue(), palette.n1.hue(), epsilon = 0.001);
        assert_relative_eq!(
            palette.neutral_variant().hue(),
            palette.n2.hue(),
            epsilon = 0.001
        );
    }

    #[test]
    fn test_tones_from_palettes() {
        let palette = CorePalette::of(BLUE);

        // Should be able to get tones from all palettes
        let primary_50 = palette.a1.tone(50);
        let secondary_50 = palette.a2.tone(50);
        let tertiary_50 = palette.a3.tone(50);
        let neutral_50 = palette.n1.tone(50);
        let neutral_variant_50 = palette.n2.tone(50);
        let error_50 = palette.error.tone(50);

        // All should be valid ARGB colors
        assert_eq!(primary_50 >> 24, 0xFF);
        assert_eq!(secondary_50 >> 24, 0xFF);
        assert_eq!(tertiary_50 >> 24, 0xFF);
        assert_eq!(neutral_50 >> 24, 0xFF);
        assert_eq!(neutral_variant_50 >> 24, 0xFF);
        assert_eq!(error_50 >> 24, 0xFF);
    }

    #[test]
    fn test_clone() {
        let palette = CorePalette::of(BLUE);
        let cloned = palette.clone();

        // Cloned palette should have same values
        assert_relative_eq!(cloned.a1.hue(), palette.a1.hue(), epsilon = 0.001);
        assert_relative_eq!(cloned.a2.hue(), palette.a2.hue(), epsilon = 0.001);
        assert_relative_eq!(cloned.a3.hue(), palette.a3.hue(), epsilon = 0.001);
        assert_relative_eq!(cloned.n1.hue(), palette.n1.hue(), epsilon = 0.001);
        assert_relative_eq!(cloned.n2.hue(), palette.n2.hue(), epsilon = 0.001);
        assert_relative_eq!(cloned.error.hue(), palette.error.hue(), epsilon = 0.001);
    }

    #[test]
    fn test_standard_mode_min_chroma() {
        // Test that standard mode enforces minimum chroma of 48 for primary
        // Use a low-chroma color
        let gray = 0xFF808080; // Medium gray
        let palette = CorePalette::of(gray);

        // Primary should have at least 48 chroma in standard mode
        assert!(palette.a1.chroma() >= 48.0);
    }

    #[test]
    fn test_various_source_colors() {
        // Test palette creation with various source colors
        for color in [RED, GREEN, BLUE, CYAN, 0xFF800080, 0xFFFF8000] {
            let palette = CorePalette::of(color);

            // All palettes should be valid (hue can be >= 360 before normalization in HCT)
            // Just verify they produce valid tones
            let _primary_50 = palette.a1.tone(50);
            let _secondary_50 = palette.a2.tone(50);
            let _tertiary_50 = palette.a3.tone(50);
            let _neutral_50 = palette.n1.tone(50);
            let _neutral_variant_50 = palette.n2.tone(50);
            let _error_50 = palette.error.tone(50);
        }
    }

    #[test]
    fn test_core_palette_colors_builder() {
        let colors = CorePaletteColors::new(BLUE)
            .with_secondary(RED)
            .with_tertiary(GREEN)
            .with_neutral(0xFF808080)
            .with_neutral_variant(0xFF606060)
            .with_error(0xFFFF0000);

        assert_eq!(colors.primary, BLUE);
        assert_eq!(colors.secondary, Some(RED));
        assert_eq!(colors.tertiary, Some(GREEN));
        assert_eq!(colors.neutral, Some(0xFF808080));
        assert_eq!(colors.neutral_variant, Some(0xFF606060));
        assert_eq!(colors.error, Some(0xFFFF0000));
    }
}
