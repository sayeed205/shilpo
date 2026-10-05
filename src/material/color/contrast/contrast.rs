//! Contrast ratio calculations for WCAG accessibility compliance.
//!
//! Provides methods to:
//! - Calculate contrast ratios between two colors (1 to 21)
//! - Find lighter tones that meet a target contrast ratio
//! - Find darker tones that meet a target contrast ratio
//!
//! Contrast ratio is calculated using XYZ's Y component. When linearized to match
//! human perception, Y becomes HCT's tone and L*a*b*'s L*. Informally, this is
//! the lightness of a color.
//!
//! Methods refer to tone (T in HCT), which is equivalent to L* in L*a*b* or L in LCH.

use crate::material::color::utils::{clamp_double, lstar_from_y, y_from_lstar};

/// Utility struct for contrast ratio calculations.
///
/// All methods are static and operate on tone values (0-100).
/// Contrast ratios range from 1 (no contrast) to 21 (maximum, black vs white).
pub struct Contrast;

impl Contrast {
    /// Returns the contrast ratio between two tones.
    ///
    /// Contrast ratio ranges from 1 to 21.
    ///
    /// # Arguments
    /// * `tone_a` - Tone between 0 and 100. Values outside will be clamped.
    /// * `tone_b` - Tone between 0 and 100. Values outside will be clamped.
    ///
    /// # Returns
    /// Contrast ratio between the two tones.
    ///
    /// # Example
    /// ```
    /// use crate::material::color::contrast::Contrast;
    ///
    /// let ratio = Contrast::ratio_of_tones(0.0, 100.0);
    /// assert!((ratio - 21.0).abs() < 0.1);
    /// ```
    pub fn ratio_of_tones(tone_a: f64, tone_b: f64) -> f64 {
        let tone_a = clamp_double(0.0, 100.0, tone_a);
        let tone_b = clamp_double(0.0, 100.0, tone_b);
        Self::ratio_of_ys(y_from_lstar(tone_a), y_from_lstar(tone_b))
    }

    /// Returns the contrast ratio between two Y (luminance) values.
    ///
    /// # Arguments
    /// * `y1` - Y luminance value
    /// * `y2` - Y luminance value
    ///
    /// # Returns
    /// Contrast ratio between the two Y values.
    pub fn ratio_of_ys(y1: f64, y2: f64) -> f64 {
        let lighter = if y1 > y2 { y1 } else { y2 };
        let darker = if lighter == y2 { y1 } else { y2 };
        (lighter + 5.0) / (darker + 5.0)
    }

    /// Returns a tone >= the input tone that ensures the target contrast ratio.
    ///
    /// Return value is between 0 and 100.
    /// Returns -1 if the ratio cannot be achieved with the input tone.
    ///
    /// # Arguments
    /// * `tone` - Tone that the return value must contrast with (0-100).
    ///   Invalid values will result in -1 being returned.
    /// * `ratio` - Target contrast ratio (1-21).
    ///   Invalid values have undefined behavior.
    ///
    /// # Returns
    /// A lighter tone that achieves the contrast ratio, or -1 if impossible.
    ///
    /// # Example
    /// ```
    /// use crate::material::color::contrast::Contrast;
    ///
    /// let lighter_tone = Contrast::lighter(50.0, 4.5);
    /// assert!(lighter_tone > 50.0 || lighter_tone == -1.0);
    /// ```
    pub fn lighter(tone: f64, ratio: f64) -> f64 {
        if !(0.0..=100.0).contains(&tone) {
            return -1.0;
        }

        let dark_y = y_from_lstar(tone);
        let light_y = ratio * (dark_y + 5.0) - 5.0;
        let real_contrast = Self::ratio_of_ys(light_y, dark_y);
        let delta = (real_contrast - ratio).abs();

        if real_contrast < ratio && delta > 0.04 {
            return -1.0;
        }

        // Ensure gamut mapping, which requires a 'range' on tone, will still result
        // in the correct ratio by lightening slightly.
        let return_value = lstar_from_y(light_y) + 0.4;

        if !(0.0..=100.0).contains(&return_value) {
            return -1.0;
        }

        return_value
    }

    /// Returns a tone <= the input tone that ensures the target contrast ratio.
    ///
    /// Return value is between 0 and 100.
    /// Returns -1 if the ratio cannot be achieved with the input tone.
    ///
    /// # Arguments
    /// * `tone` - Tone that the return value must contrast with (0-100).
    ///   Invalid values will result in -1 being returned.
    /// * `ratio` - Target contrast ratio (1-21).
    ///   Invalid values have undefined behavior.
    ///
    /// # Returns
    /// A darker tone that achieves the contrast ratio, or -1 if impossible.
    ///
    /// # Example
    /// ```
    /// use crate::material::color::contrast::Contrast;
    ///
    /// let darker_tone = Contrast::darker(50.0, 4.5);
    /// assert!(darker_tone < 50.0 || darker_tone == -1.0);
    /// ```
    pub fn darker(tone: f64, ratio: f64) -> f64 {
        if !(0.0..=100.0).contains(&tone) {
            return -1.0;
        }

        let light_y = y_from_lstar(tone);
        let dark_y = ((light_y + 5.0) / ratio) - 5.0;
        let real_contrast = Self::ratio_of_ys(light_y, dark_y);
        let delta = (real_contrast - ratio).abs();

        if real_contrast < ratio && delta > 0.04 {
            return -1.0;
        }

        // Ensure gamut mapping, which requires a 'range' on tone, will still result
        // in the correct ratio by darkening slightly.
        let return_value = lstar_from_y(dark_y) - 0.4;

        if !(0.0..=100.0).contains(&return_value) {
            return -1.0;
        }

        return_value
    }

    /// Returns a tone >= the input tone that ensures the target contrast ratio.
    ///
    /// Return value is between 0 and 100.
    /// Returns 100 if the ratio cannot be achieved with the input tone.
    ///
    /// This method clamps the result to valid tone bounds (0-100), but the
    /// returned value may not actually achieve the target ratio.
    /// For example, there is no color lighter than T100.
    ///
    /// # Arguments
    /// * `tone` - Tone that the return value must contrast with (0-100).
    ///   Invalid values will result in 100 being returned.
    /// * `ratio` - Target contrast ratio (1-21).
    ///   Invalid values have undefined behavior.
    ///
    /// # Returns
    /// A lighter tone, or 100 if the contrast ratio cannot be achieved.
    pub fn lighter_clamped(tone: f64, ratio: f64) -> f64 {
        let lighter_safe = Self::lighter(tone, ratio);
        if lighter_safe < 0.0 {
            100.0
        } else {
            lighter_safe
        }
    }

    /// Returns a tone <= the input tone that ensures the target contrast ratio.
    ///
    /// Return value is between 0 and 100.
    /// Returns 0 if the ratio cannot be achieved with the input tone.
    ///
    /// This method clamps the result to valid tone bounds (0-100), but the
    /// returned value may not actually achieve the target ratio.
    /// For example, there is no color darker than T0.
    ///
    /// # Arguments
    /// * `tone` - Tone that the return value must contrast with (0-100).
    ///   Invalid values will result in 0 being returned.
    /// * `ratio` - Target contrast ratio (1-21).
    ///   Invalid values have undefined behavior.
    ///
    /// # Returns
    /// A darker tone, or 0 if the contrast ratio cannot be achieved.
    pub fn darker_clamped(tone: f64, ratio: f64) -> f64 {
        let darker_safe = Self::darker(tone, ratio);
        if darker_safe < 0.0 { 0.0 } else { darker_safe }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ratio_of_tones Tests ====================

    #[test]
    fn test_ratio_of_tones_black_white() {
        // Maximum contrast: black (0) vs white (100)
        let ratio = Contrast::ratio_of_tones(0.0, 100.0);
        assert!(
            (ratio - 21.0).abs() < 0.5,
            "Black vs white should be ~21, got {}",
            ratio
        );
    }

    #[test]
    fn test_ratio_of_tones_same_tone() {
        // Same tone = no contrast (ratio 1)
        let ratio = Contrast::ratio_of_tones(50.0, 50.0);
        assert!(
            (ratio - 1.0).abs() < 0.01,
            "Same tone should have ratio 1, got {}",
            ratio
        );
    }

    #[test]
    fn test_ratio_of_tones_symmetric() {
        // Ratio should be the same regardless of order
        let ratio_ab = Contrast::ratio_of_tones(30.0, 80.0);
        let ratio_ba = Contrast::ratio_of_tones(80.0, 30.0);
        assert!(
            (ratio_ab - ratio_ba).abs() < 0.001,
            "Ratio should be symmetric"
        );
    }

    #[test]
    fn test_ratio_of_tones_clamped_negative() {
        // Negative values should be clamped to 0
        let ratio = Contrast::ratio_of_tones(-10.0, 100.0);
        let ratio_clamped = Contrast::ratio_of_tones(0.0, 100.0);
        assert!((ratio - ratio_clamped).abs() < 0.001);
    }

    #[test]
    fn test_ratio_of_tones_clamped_above() {
        // Values > 100 should be clamped to 100
        let ratio = Contrast::ratio_of_tones(0.0, 150.0);
        let ratio_clamped = Contrast::ratio_of_tones(0.0, 100.0);
        assert!((ratio - ratio_clamped).abs() < 0.001);
    }

    #[test]
    fn test_ratio_of_tones_wcag_aa_large_text() {
        // WCAG AA for large text requires 3:1 contrast
        // Tone 50 vs 0 should exceed 3:1
        let ratio = Contrast::ratio_of_tones(50.0, 0.0);
        assert!(
            ratio >= 3.0,
            "50 vs 0 should have ratio >= 3:1, got {}",
            ratio
        );
    }

    #[test]
    fn test_ratio_of_tones_midtones() {
        // Two mid-range tones should have lower contrast
        let ratio = Contrast::ratio_of_tones(40.0, 60.0);
        assert!(
            ratio > 1.0 && ratio < 5.0,
            "Mid-range tones should have moderate contrast"
        );
    }

    // ==================== ratio_of_ys Tests ====================

    #[test]
    fn test_ratio_of_ys_black_white() {
        // Y=0 (black) vs Y=100 (white)
        let ratio = Contrast::ratio_of_ys(0.0, 100.0);
        assert!((ratio - 21.0).abs() < 0.01);
    }

    #[test]
    fn test_ratio_of_ys_same() {
        let ratio = Contrast::ratio_of_ys(50.0, 50.0);
        assert!((ratio - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_ratio_of_ys_symmetric() {
        let ratio_ab = Contrast::ratio_of_ys(10.0, 50.0);
        let ratio_ba = Contrast::ratio_of_ys(50.0, 10.0);
        assert!((ratio_ab - ratio_ba).abs() < 0.001);
    }

    #[test]
    fn test_ratio_of_ys_formula() {
        // Verify the formula: (lighter + 5) / (darker + 5)
        let y1 = 20.0;
        let y2 = 80.0;
        let expected = (80.0 + 5.0) / (20.0 + 5.0);
        let actual = Contrast::ratio_of_ys(y1, y2);
        assert!((actual - expected).abs() < 0.001);
    }

    // ==================== lighter Tests ====================

    #[test]
    fn test_lighter_achievable() {
        // From a dark tone, we should be able to find a lighter tone
        let result = Contrast::lighter(30.0, 3.0);
        assert!(result > 0.0, "Should find a lighter tone, got {}", result);
        assert!(result > 30.0, "Result should be lighter than input");
    }

    #[test]
    fn test_lighter_invalid_tone_negative() {
        let result = Contrast::lighter(-5.0, 3.0);
        assert_eq!(result, -1.0, "Invalid negative tone should return -1");
    }

    #[test]
    fn test_lighter_invalid_tone_above() {
        let result = Contrast::lighter(105.0, 3.0);
        assert_eq!(result, -1.0, "Invalid tone > 100 should return -1");
    }

    #[test]
    fn test_lighter_impossible_from_white() {
        // Cannot go lighter than white
        let result = Contrast::lighter(100.0, 4.5);
        assert_eq!(result, -1.0, "Cannot find lighter than white");
    }

    #[test]
    fn test_lighter_near_white() {
        // Very high tones may not be able to achieve high contrast going lighter
        let result = Contrast::lighter(95.0, 4.5);
        assert_eq!(result, -1.0, "High contrast from near-white is impossible");
    }

    #[test]
    fn test_lighter_ratio_1() {
        // Ratio of 1 means no change needed
        let result = Contrast::lighter(50.0, 1.0);
        // Should return something >= 50 (with small adjustment)
        assert!(
            result >= 49.0,
            "Ratio 1 should return close to input, got {}",
            result
        );
    }

    #[test]
    fn test_lighter_wcag_aa() {
        // Find a lighter tone for WCAG AA (4.5:1) from tone 30
        let result = Contrast::lighter(30.0, 4.5);
        if result > 0.0 {
            let actual_ratio = Contrast::ratio_of_tones(30.0, result);
            assert!(
                actual_ratio >= 4.3,
                "Should achieve ~4.5:1 ratio, got {}",
                actual_ratio
            );
        }
    }

    #[test]
    fn test_lighter_returns_bounded_value() {
        let result = Contrast::lighter(20.0, 3.0);
        if result > 0.0 {
            assert!(
                (0.0..=100.0).contains(&result),
                "Result should be in [0, 100]"
            );
        }
    }

    // ==================== darker Tests ====================

    #[test]
    fn test_darker_achievable() {
        // From a light tone, we should be able to find a darker tone
        let result = Contrast::darker(70.0, 3.0);
        assert!(result > 0.0 || result == -1.0);
        if result > 0.0 {
            assert!(result < 70.0, "Result should be darker than input");
        }
    }

    #[test]
    fn test_darker_invalid_tone_negative() {
        let result = Contrast::darker(-5.0, 3.0);
        assert_eq!(result, -1.0, "Invalid negative tone should return -1");
    }

    #[test]
    fn test_darker_invalid_tone_above() {
        let result = Contrast::darker(105.0, 3.0);
        assert_eq!(result, -1.0, "Invalid tone > 100 should return -1");
    }

    #[test]
    fn test_darker_impossible_from_black() {
        // Cannot go darker than black
        let result = Contrast::darker(0.0, 4.5);
        assert_eq!(result, -1.0, "Cannot find darker than black");
    }

    #[test]
    fn test_darker_near_black() {
        // Very low tones may not be able to achieve high contrast going darker
        let result = Contrast::darker(5.0, 4.5);
        assert_eq!(result, -1.0, "High contrast from near-black is impossible");
    }

    #[test]
    fn test_darker_ratio_1() {
        // Ratio of 1 means no change needed
        let result = Contrast::darker(50.0, 1.0);
        // Should return something <= 50 (with small adjustment)
        if result > 0.0 {
            assert!(
                result <= 51.0,
                "Ratio 1 should return close to input, got {}",
                result
            );
        }
    }

    #[test]
    fn test_darker_wcag_aa() {
        // Find a darker tone for WCAG AA (4.5:1) from tone 80
        let result = Contrast::darker(80.0, 4.5);
        if result > 0.0 {
            let actual_ratio = Contrast::ratio_of_tones(80.0, result);
            assert!(
                actual_ratio >= 4.3,
                "Should achieve ~4.5:1 ratio, got {}",
                actual_ratio
            );
        }
    }

    #[test]
    fn test_darker_returns_bounded_value() {
        let result = Contrast::darker(80.0, 3.0);
        if result > 0.0 {
            assert!(
                (0.0..=100.0).contains(&result),
                "Result should be in [0, 100]"
            );
        }
    }

    // ==================== lighter_clamped Tests ====================

    #[test]
    fn test_lighter_clamped_achievable() {
        let result = Contrast::lighter_clamped(30.0, 3.0);
        assert!((0.0..=100.0).contains(&result));
        assert!(result > 30.0, "Should return lighter tone");
    }

    #[test]
    fn test_lighter_clamped_impossible_returns_100() {
        // From near-white with high ratio requirement, should return 100
        let result = Contrast::lighter_clamped(100.0, 4.5);
        assert_eq!(result, 100.0, "Impossible lighter should return 100");
    }

    #[test]
    fn test_lighter_clamped_invalid_tone_returns_100() {
        let result = Contrast::lighter_clamped(-5.0, 3.0);
        assert_eq!(result, 100.0, "Invalid tone should return 100");
    }

    #[test]
    fn test_lighter_clamped_always_bounded() {
        // Test various inputs to ensure result is always in [0, 100]
        for tone in [0.0, 25.0, 50.0, 75.0, 100.0] {
            for ratio in [1.0, 3.0, 4.5, 7.0, 21.0] {
                let result = Contrast::lighter_clamped(tone, ratio);
                assert!(
                    (0.0..=100.0).contains(&result),
                    "Result should be bounded for tone={}, ratio={}",
                    tone,
                    ratio
                );
            }
        }
    }

    // ==================== darker_clamped Tests ====================

    #[test]
    fn test_darker_clamped_achievable() {
        let result = Contrast::darker_clamped(70.0, 3.0);
        assert!((0.0..=100.0).contains(&result));
        if result > 0.0 {
            assert!(result < 70.0, "Should return darker tone");
        }
    }

    #[test]
    fn test_darker_clamped_impossible_returns_0() {
        // From near-black with high ratio requirement, should return 0
        let result = Contrast::darker_clamped(0.0, 4.5);
        assert_eq!(result, 0.0, "Impossible darker should return 0");
    }

    #[test]
    fn test_darker_clamped_invalid_tone_returns_0() {
        let result = Contrast::darker_clamped(-5.0, 3.0);
        assert_eq!(result, 0.0, "Invalid tone should return 0");
    }

    #[test]
    fn test_darker_clamped_always_bounded() {
        // Test various inputs to ensure result is always in [0, 100]
        for tone in [0.0, 25.0, 50.0, 75.0, 100.0] {
            for ratio in [1.0, 3.0, 4.5, 7.0, 21.0] {
                let result = Contrast::darker_clamped(tone, ratio);
                assert!(
                    (0.0..=100.0).contains(&result),
                    "Result should be bounded for tone={}, ratio={}",
                    tone,
                    ratio
                );
            }
        }
    }

    // ==================== WCAG Compliance Tests ====================

    #[test]
    fn test_wcag_aa_normal_text() {
        // WCAG AA requires 4.5:1 for normal text
        let lighter = Contrast::lighter(25.0, 4.5);
        if lighter > 0.0 {
            let ratio = Contrast::ratio_of_tones(25.0, lighter);
            assert!(ratio >= 4.4, "AA compliance requires 4.5:1, got {}", ratio);
        }
    }

    #[test]
    fn test_wcag_aa_large_text() {
        // WCAG AA requires 3:1 for large text
        let lighter = Contrast::lighter(40.0, 3.0);
        if lighter > 0.0 {
            let ratio = Contrast::ratio_of_tones(40.0, lighter);
            assert!(ratio >= 2.9, "AA large text requires 3:1, got {}", ratio);
        }
    }

    #[test]
    fn test_wcag_aaa_normal_text() {
        // WCAG AAA requires 7:1 for normal text
        let lighter = Contrast::lighter(15.0, 7.0);
        if lighter > 0.0 {
            let ratio = Contrast::ratio_of_tones(15.0, lighter);
            assert!(ratio >= 6.8, "AAA compliance requires 7:1, got {}", ratio);
        }
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_tone_boundaries() {
        // Test exact boundaries
        assert!(Contrast::lighter(0.0, 3.0) > 0.0, "Should work at tone 0");
        assert!(
            Contrast::darker(100.0, 3.0) > 0.0,
            "Should work at tone 100"
        );
    }

    #[test]
    fn test_minimal_contrast_lighter() {
        // With ratio close to 1, result should be close to input
        let result = Contrast::lighter(50.0, 1.001);
        if result > 0.0 {
            assert!(
                (result - 50.0).abs() < 5.0,
                "Minimal ratio should give close result"
            );
        }
    }

    #[test]
    fn test_minimal_contrast_darker() {
        // With ratio close to 1, result should be close to input
        let result = Contrast::darker(50.0, 1.001);
        if result > 0.0 {
            assert!(
                (result - 50.0).abs() < 5.0,
                "Minimal ratio should give close result"
            );
        }
    }

    #[test]
    fn test_maximum_contrast() {
        // Maximum achievable contrast is ~21:1 (black vs white)
        let ratio = Contrast::ratio_of_tones(0.0, 100.0);
        assert!(ratio > 20.0 && ratio <= 21.0, "Max contrast should be ~21");
    }

    #[test]
    fn test_lighter_darker_complementary() {
        // lighter(dark_tone, ratio) and darker(light_tone, ratio) should find
        // complementary tones that achieve similar contrast
        let base_dark = 25.0;
        let base_light = 75.0;
        let ratio = 3.0;

        let lighter_result = Contrast::lighter(base_dark, ratio);
        let darker_result = Contrast::darker(base_light, ratio);

        if lighter_result > 0.0 && darker_result > 0.0 {
            let ratio1 = Contrast::ratio_of_tones(base_dark, lighter_result);
            let ratio2 = Contrast::ratio_of_tones(base_light, darker_result);
            // Both should achieve approximately the requested ratio
            assert!((ratio1 - ratio).abs() < 0.5, "Lighter should achieve ratio");
            assert!((ratio2 - ratio).abs() < 0.5, "Darker should achieve ratio");
        }
    }
}
