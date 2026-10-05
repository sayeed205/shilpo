use std::collections::HashMap;
use std::sync::RwLock;

use crate::material::color::hct::Hct;

/// A convenience class for retrieving colors that are constant in hue and
/// chroma, but vary in tone.
///
/// TonalPalette generates colors at any tone (0-100) while maintaining
/// consistent hue and chroma. This is useful for creating color schemes
/// where you need multiple shades of the same color.
#[derive(Debug)]
pub struct TonalPalette {
    hue: f64,
    chroma: f64,
    key_color: Hct,
    cache: RwLock<HashMap<i32, u32>>,
}

impl TonalPalette {
    /// Create a TonalPalette from an ARGB integer.
    ///
    /// # Arguments
    ///
    /// * `argb` - ARGB representation of a color
    ///
    /// # Returns
    ///
    /// Tones matching that color's hue and chroma.
    pub fn from_int(argb: u32) -> Self {
        let hct = Hct::from_int(argb);
        Self::from_hct(hct)
    }

    /// Create a TonalPalette from an HCT color.
    ///
    /// # Arguments
    ///
    /// * `hct` - HCT color
    ///
    /// # Returns
    ///
    /// Tones matching that color's hue and chroma.
    pub(crate) fn from_hct(hct: Hct) -> Self {
        Self {
            hue: hct.hue(),
            chroma: hct.chroma(),
            key_color: hct,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Create a TonalPalette from hue and chroma values.
    ///
    /// # Arguments
    ///
    /// * `hue` - HCT hue (0-360)
    /// * `chroma` - HCT chroma
    ///
    /// # Returns
    ///
    /// Tones matching the given hue and chroma.
    pub fn from_hue_and_chroma(hue: f64, chroma: f64) -> Self {
        let key_color = KeyColor::new(hue, chroma).create();
        Self {
            hue,
            chroma,
            key_color,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Get the ARGB color at a specific tone.
    ///
    /// # Arguments
    ///
    /// * `tone` - HCT tone, measured from 0 to 100.
    ///
    /// # Returns
    ///
    /// ARGB representation of a color with that tone.
    pub fn tone(&self, tone: impl Into<f64>) -> u32 {
        self.tone_at(tone.into())
    }

    /// Get the ARGB color at an exact fractional tone. Integral inputs retain
    /// the palette's canonical cache and yellow T99 special case; non-integral
    /// inputs are sent to HCT without rounding or truncation.
    pub fn tone_at(&self, tone: f64) -> u32 {
        if tone.is_finite()
            && tone.fract() == 0.0
            && tone >= i32::MIN as f64
            && tone <= i32::MAX as f64
        {
            self.tone_integer(tone as i32)
        } else {
            Hct::from(self.hue, self.chroma, tone).to_int()
        }
    }

    fn tone_integer(&self, tone: i32) -> u32 {
        // Check cache first (read lock)
        // Recover from poisoned lock - the data is still valid even if a previous thread panicked
        {
            let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
            if let Some(&argb) = cache.get(&tone) {
                return argb;
            }
        }

        // Calculate and cache (write lock)
        let argb = if tone == 99 && Hct::is_yellow(self.hue) {
            // Special handling for yellow at tone 99
            self.average_argb(self.tone(98), self.tone(100))
        } else {
            Hct::from(self.hue, self.chroma, tone as f64).to_int()
        };

        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        cache.insert(tone, argb);
        argb
    }

    /// Get the HCT color at a specific tone.
    ///
    /// # Arguments
    ///
    /// * `tone` - HCT tone, measured from 0 to 100.
    ///
    /// # Returns
    ///
    /// HCT representation of a color with that tone.
    pub(crate) fn get_hct(&self, tone: impl Into<f64>) -> Hct {
        Hct::from_int(self.tone(tone))
    }

    /// Get the hue of this palette.
    pub fn hue(&self) -> f64 {
        self.hue
    }

    /// Get the chroma of this palette.
    pub fn chroma(&self) -> f64 {
        self.chroma
    }

    /// Get the key color of this palette.
    pub(crate) fn key_color(&self) -> Hct {
        self.key_color
    }

    /// Get the key color as an ARGB integer.
    pub fn key_color_argb(&self) -> u32 {
        self.key_color.to_int()
    }

    /// Average two ARGB colors.
    fn average_argb(&self, argb1: u32, argb2: u32) -> u32 {
        let red1 = (argb1 >> 16) & 0xff;
        let green1 = (argb1 >> 8) & 0xff;
        let blue1 = argb1 & 0xff;
        let red2 = (argb2 >> 16) & 0xff;
        let green2 = (argb2 >> 8) & 0xff;
        let blue2 = argb2 & 0xff;
        // Round each channel midpoint to the nearest integer.
        let red = ((red1 as f64 + red2 as f64) / 2.0).round() as u32;
        let green = ((green1 as f64 + green2 as f64) / 2.0).round() as u32;
        let blue = ((blue1 as f64 + blue2 as f64) / 2.0).round() as u32;
        0xff000000 | (red << 16) | (green << 8) | blue
    }
}

impl Clone for TonalPalette {
    fn clone(&self) -> Self {
        // Recover from poisoned lock - the data is still valid even if a previous thread panicked
        Self {
            hue: self.hue,
            chroma: self.chroma,
            key_color: self.key_color,
            cache: RwLock::new(self.cache.read().unwrap_or_else(|e| e.into_inner()).clone()),
        }
    }
}

/// Key color is a color that represents the hue and chroma of a tonal palette.
///
/// The key color is the first tone, starting from T50, matching the given hue
/// and chroma. It's used as a representative color for the palette.
struct KeyColor {
    hue: f64,
    requested_chroma: f64,
    chroma_cache: HashMap<i32, f64>,
}

impl KeyColor {
    const MAX_CHROMA_VALUE: f64 = 200.0;

    fn new(hue: f64, requested_chroma: f64) -> Self {
        Self {
            hue,
            requested_chroma,
            chroma_cache: HashMap::new(),
        }
    }

    /// Creates a key color from hue and chroma.
    ///
    /// The key color is the first tone, starting from T50, matching the given
    /// hue and chroma.
    fn create(&mut self) -> Hct {
        // Pivot around T50 because T50 has the most chroma available, on
        // average. Thus it is most likely to have a direct answer.
        let pivot_tone = 50;
        let tone_step_size = 1;
        // Epsilon to accept values slightly higher than the requested chroma.
        let epsilon = 0.01;

        // Binary search to find the tone that can provide a chroma that is closest
        // to the requested chroma.
        let mut lower_tone = 0;
        let mut upper_tone = 100;

        while lower_tone < upper_tone {
            let mid_tone = (lower_tone + upper_tone) / 2;
            let is_ascending =
                self.max_chroma(mid_tone) < self.max_chroma(mid_tone + tone_step_size);
            let sufficient_chroma = self.max_chroma(mid_tone) >= self.requested_chroma - epsilon;

            if sufficient_chroma {
                // Either range [lower_tone, mid_tone] or [mid_tone, upper_tone] has
                // the answer, so search in the range that is closer to the pivot tone.
                if (lower_tone - pivot_tone).abs() < (upper_tone - pivot_tone).abs() {
                    upper_tone = mid_tone;
                } else {
                    if lower_tone == mid_tone {
                        return Hct::from(self.hue, self.requested_chroma, lower_tone as f64);
                    }
                    lower_tone = mid_tone;
                }
            } else {
                // As there is no sufficient chroma in the mid_tone, follow the direction
                // to the chroma peak.
                if is_ascending {
                    lower_tone = mid_tone + tone_step_size;
                } else {
                    // Keep mid_tone for potential chroma peak.
                    upper_tone = mid_tone;
                }
            }
        }

        Hct::from(self.hue, self.requested_chroma, lower_tone as f64)
    }

    /// Find the maximum chroma for a given tone.
    fn max_chroma(&mut self, tone: i32) -> f64 {
        if let Some(&chroma) = self.chroma_cache.get(&tone) {
            return chroma;
        }
        let chroma = Hct::from(self.hue, Self::MAX_CHROMA_VALUE, tone as f64).chroma();
        self.chroma_cache.insert(tone, chroma);
        chroma
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

    #[test]
    fn test_from_int_creates_palette() {
        let palette = TonalPalette::from_int(RED);
        // Red has hue around 27
        assert_relative_eq!(palette.hue(), 27.408, epsilon = 1.0);
        assert!(palette.chroma() > 100.0);
    }

    #[test]
    fn test_from_hue_and_chroma_creates_palette() {
        let palette = TonalPalette::from_hue_and_chroma(180.0, 50.0);
        assert_relative_eq!(palette.hue(), 180.0, epsilon = 0.001);
        assert_relative_eq!(palette.chroma(), 50.0, epsilon = 0.001);
    }

    #[test]
    fn test_tone_returns_argb() {
        let palette = TonalPalette::from_hue_and_chroma(180.0, 50.0);

        // Tone 0 should be black
        let tone_0 = palette.tone(0);
        let hct_0 = Hct::from_int(tone_0);
        assert_relative_eq!(hct_0.tone(), 0.0, epsilon = 1.0);

        // Tone 100 should be white
        let tone_100 = palette.tone(100);
        let hct_100 = Hct::from_int(tone_100);
        assert_relative_eq!(hct_100.tone(), 100.0, epsilon = 1.0);

        // Tone 50 should be mid-range
        let tone_50 = palette.tone(50);
        let hct_50 = Hct::from_int(tone_50);
        assert_relative_eq!(hct_50.tone(), 50.0, epsilon = 1.0);
    }

    #[test]
    fn test_tone_preserves_hue() {
        let palette = TonalPalette::from_hue_and_chroma(180.0, 50.0);

        for tone in (10..=90).step_by(10) {
            let argb = palette.tone(tone);
            let hct = Hct::from_int(argb);
            // Hue should be close (may shift at very low chroma)
            if hct.chroma() > 5.0 {
                let hue_diff = (hct.hue() - 180.0).abs();
                let hue_diff = hue_diff.min(360.0 - hue_diff);
                assert!(hue_diff < 10.0, "Hue diff {} at tone {}", hue_diff, tone);
            }
        }
    }

    #[test]
    fn test_tone_caching() {
        let palette = TonalPalette::from_hue_and_chroma(180.0, 50.0);

        // First call
        let argb1 = palette.tone(50);
        // Second call should return same value (from cache)
        let argb2 = palette.tone(50);

        assert_eq!(argb1, argb2);
    }

    #[test]
    fn test_get_hct() {
        let palette = TonalPalette::from_hue_and_chroma(180.0, 50.0);
        let hct = palette.get_hct(50);

        assert_relative_eq!(hct.tone(), 50.0, epsilon = 1.0);
    }

    #[test]
    fn test_from_hct() {
        let original_hct = Hct::from(180.0, 50.0, 50.0);
        let palette = TonalPalette::from_hct(original_hct);

        assert_relative_eq!(palette.hue(), original_hct.hue(), epsilon = 0.001);
        assert_relative_eq!(palette.chroma(), original_hct.chroma(), epsilon = 0.001);
    }

    #[test]
    fn test_key_color() {
        let palette = TonalPalette::from_hue_and_chroma(180.0, 50.0);
        let key_color = palette.key_color();

        // Key color should have the correct hue
        let hue_diff = (key_color.hue() - 180.0).abs();
        let hue_diff = hue_diff.min(360.0 - hue_diff);
        assert!(hue_diff < 10.0);
    }

    #[test]
    fn test_key_color_pivot_ties_and_peak_chroma_match_reference() {
        // These choices characterize the nearest-to-T50 tie direction, the
        // achievable-chroma threshold, and the maximum-chroma fallback.
        assert_eq!(
            TonalPalette::from_hue_and_chroma(50.0, 60.0)
                .key_color()
                .to_int(),
            0xffc65e03,
        );
        assert_eq!(
            TonalPalette::from_hue_and_chroma(149.0, 200.0)
                .key_color()
                .to_int(),
            0xff00fe69,
        );
        assert_eq!(
            TonalPalette::from_hue_and_chroma(50.0, 3.0)
                .key_color()
                .to_int(),
            0xff7d7672,
        );
    }

    #[test]
    fn test_canonical_blue_tones_are_exact() {
        let palette = TonalPalette::from_int(BLUE);

        for (tone, expected) in [
            (100, 0xffffffff),
            (95, 0xfff1efff),
            (90, 0xffe0e0ff),
            (80, 0xffbec2ff),
            (70, 0xff9da3ff),
            (60, 0xff7c84ff),
            (50, 0xff5a64ff),
            (40, 0xff343dff),
            (30, 0xff0000ef),
            (20, 0xff0001ac),
            (10, 0xff00006e),
            (0, 0xff000000),
        ] {
            assert_eq!(palette.tone(tone), expected, "tone {tone}");
        }
    }

    #[test]
    fn test_clone() {
        let palette = TonalPalette::from_hue_and_chroma(180.0, 50.0);
        // Populate cache
        let _tone_50 = palette.tone(50);

        let cloned = palette.clone();
        assert_relative_eq!(cloned.hue(), palette.hue(), epsilon = 0.001);
        assert_relative_eq!(cloned.chroma(), palette.chroma(), epsilon = 0.001);

        // Cloned palette should also return same tone
        assert_eq!(cloned.tone(50), palette.tone(50));
    }

    #[test]
    fn test_various_hues() {
        // Test palette creation at various hues
        for hue in (0..360).step_by(30) {
            let palette = TonalPalette::from_hue_and_chroma(hue as f64, 50.0);
            assert_relative_eq!(palette.hue(), hue as f64, epsilon = 0.001);

            // Should be able to get tones without panicking
            let _tone_50 = palette.tone(50);
        }
    }

    #[test]
    fn test_various_chromas() {
        // Test palette creation at various chromas
        for chroma in [0.0, 10.0, 25.0, 50.0, 75.0, 100.0, 150.0] {
            let palette = TonalPalette::from_hue_and_chroma(180.0, chroma);
            assert_relative_eq!(palette.chroma(), chroma, epsilon = 0.001);

            // Should be able to get tones without panicking
            let _tone_50 = palette.tone(50);
        }
    }

    #[test]
    fn test_yellow_tone_99_special_case() {
        // Yellow hue is between 105 and 125
        let yellow_palette = TonalPalette::from_hue_and_chroma(115.0, 50.0);

        let tone_98 = yellow_palette.tone(98);
        let tone_99 = yellow_palette.tone(99);
        let tone_100 = yellow_palette.tone(100);

        assert_eq!(tone_98, 0xfffaffae);
        assert_eq!(tone_99, 0xfffdffd7);
        assert_eq!(tone_100, 0xffffffff);

        let hct = Hct::from_int(tone_99);

        // Tone should be close to 99
        assert!(hct.tone() > 95.0 && hct.tone() <= 100.0);
    }

    #[test]
    fn test_all_tones_valid() {
        let palette = TonalPalette::from_hue_and_chroma(180.0, 50.0);

        // All tones from 0 to 100 should produce valid colors
        for tone in 0..=100 {
            let argb = palette.tone(tone);
            // Should be a valid ARGB with alpha = 255
            assert_eq!(argb >> 24, 0xFF, "Invalid alpha at tone {}", tone);
        }
    }

    #[test]
    fn test_from_int_red() {
        let palette = TonalPalette::from_int(RED);

        // Tone 50 should produce a color with similar hue
        let tone_50 = palette.tone(50);
        let hct = Hct::from_int(tone_50);

        // Should have red hue (around 27)
        assert!(hct.hue() < 40.0 || hct.hue() > 350.0);
    }

    #[test]
    fn test_from_int_green() {
        let palette = TonalPalette::from_int(GREEN);

        let tone_50 = palette.tone(50);
        let hct = Hct::from_int(tone_50);

        // Should have green hue (around 142)
        assert!(hct.hue() > 120.0 && hct.hue() < 160.0);
    }

    #[test]
    fn test_from_int_blue() {
        let palette = TonalPalette::from_int(BLUE);

        let tone_50 = palette.tone(50);
        let hct = Hct::from_int(tone_50);

        // Should have blue hue (around 282)
        assert!(hct.hue() > 260.0 && hct.hue() < 300.0);
    }
}
