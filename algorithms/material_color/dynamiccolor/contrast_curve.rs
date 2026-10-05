// <FILE>crates/mcu-dynamiccolor/src/contrast_curve.rs</FILE> - <DESC>Contrast curve for dynamic color selection</DESC>
// <VERS>VERSION: 1.1.0</VERS>
// <WCTX>Port ContrastCurve struct from TypeScript Material Color Utilities</WCTX>
// <CLOG>Implement ContrastCurve with 4-level interpolation and comprehensive tests</CLOG>

use crate::material_color::utils::math::lerp;

/// A contrast curve for dynamic color selection.
///
/// Contains a value that changes with the contrast level.
/// Usually represents the contrast requirements for a dynamic color on its background.
/// The four values correspond to values for contrast levels -1.0, 0.0, 0.5, and 1.0, respectively.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContrastCurve {
    /// Value for contrast level -1.0
    pub low: f64,
    /// Value for contrast level 0.0
    pub normal: f64,
    /// Value for contrast level 0.5
    pub medium: f64,
    /// Value for contrast level 1.0
    pub high: f64,
}

impl ContrastCurve {
    /// Creates a new `ContrastCurve` with values for each contrast level.
    ///
    /// # Arguments
    ///
    /// * `low` - Value for contrast level -1.0
    /// * `normal` - Value for contrast level 0.0
    /// * `medium` - Value for contrast level 0.5
    /// * `high` - Value for contrast level 1.0
    ///
    /// # Example
    ///
    /// ```
    /// use mcu_dynamiccolor::ContrastCurve;
    ///
    /// let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
    /// assert_eq!(curve.low, 1.0);
    /// assert_eq!(curve.normal, 4.5);
    /// assert_eq!(curve.medium, 7.0);
    /// assert_eq!(curve.high, 11.0);
    /// ```
    pub fn new(low: f64, normal: f64, medium: f64, high: f64) -> Self {
        ContrastCurve {
            low,
            normal,
            medium,
            high,
        }
    }

    /// Returns the value at a given contrast level using linear interpolation.
    ///
    /// # Arguments
    ///
    /// * `contrast_level` - The contrast level. 0.0 is the default (normal); -1.0 is the lowest; 1.0 is the highest.
    ///
    /// # Returns
    ///
    /// The interpolated value at the given contrast level.
    ///
    /// # Interpolation Ranges
    ///
    /// - `contrast_level <= -1.0`: returns `low`
    /// - `-1.0 < contrast_level < 0.0`: linear interpolation between `low` and `normal`
    /// - `0.0 <= contrast_level < 0.5`: linear interpolation between `normal` and `medium`
    /// - `0.5 <= contrast_level < 1.0`: linear interpolation between `medium` and `high`
    /// - `contrast_level >= 1.0`: returns `high`
    ///
    /// # Example
    ///
    /// ```
    /// use mcu_dynamiccolor::ContrastCurve;
    ///
    /// let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
    /// assert!((curve.get(-1.0) - 1.0).abs() < 1e-10);
    /// assert!((curve.get(0.0) - 4.5).abs() < 1e-10);
    /// assert!((curve.get(0.5) - 7.0).abs() < 1e-10);
    /// assert!((curve.get(1.0) - 11.0).abs() < 1e-10);
    /// ```
    pub fn get(&self, contrast_level: f64) -> f64 {
        if contrast_level <= -1.0 {
            self.low
        } else if contrast_level < 0.0 {
            // Interpolate between low and normal
            // range: [-1.0, 0.0] which spans 1.0 units
            let t = (contrast_level - (-1.0)) / 1.0;
            lerp(self.low, self.normal, t)
        } else if contrast_level < 0.5 {
            // Interpolate between normal and medium
            // range: [0.0, 0.5] which spans 0.5 units
            let t = (contrast_level - 0.0) / 0.5;
            lerp(self.normal, self.medium, t)
        } else if contrast_level < 1.0 {
            // Interpolate between medium and high
            // range: [0.5, 1.0] which spans 0.5 units
            let t = (contrast_level - 0.5) / 0.5;
            lerp(self.medium, self.high, t)
        } else {
            self.high
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        assert_eq!(curve.low, 1.0);
        assert_eq!(curve.normal, 4.5);
        assert_eq!(curve.medium, 7.0);
        assert_eq!(curve.high, 11.0);
    }

    #[test]
    fn test_debug() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let debug_str = format!("{:?}", curve);
        assert!(debug_str.contains("ContrastCurve"));
    }

    #[test]
    fn test_clone() {
        let curve1 = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let curve2 = curve1; // Copy trait, clone is implicit
        assert_eq!(curve1, curve2);
    }

    #[test]
    fn test_copy() {
        let curve1 = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let curve2 = curve1;
        assert_eq!(curve1, curve2);
    }

    #[test]
    fn test_partial_eq() {
        let curve1 = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let curve2 = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let curve3 = ContrastCurve::new(1.0, 4.5, 7.0, 12.0);
        assert_eq!(curve1, curve2);
        assert_ne!(curve1, curve3);
    }

    // Boundary cases
    #[test]
    fn test_get_at_low_boundary() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(-1.0);
        assert!((result - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_get_at_normal_boundary() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(0.0);
        assert!((result - 4.5).abs() < 1e-10);
    }

    #[test]
    fn test_get_at_medium_boundary() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(0.5);
        assert!((result - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_get_at_high_boundary() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(1.0);
        assert!((result - 11.0).abs() < 1e-10);
    }

    // Below -1.0
    #[test]
    fn test_get_below_low_boundary() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(-2.0);
        assert!((result - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_get_far_below_low_boundary() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(-100.0);
        assert!((result - 1.0).abs() < 1e-10);
    }

    // Range: -1.0 < level < 0.0 (low -> normal interpolation)
    #[test]
    fn test_get_quarter_way_low_to_normal() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(-0.75);
        let expected = 1.0 + (4.5 - 1.0) * 0.25;
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn test_get_midway_low_to_normal() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(-0.5);
        let expected = 1.0 + (4.5 - 1.0) * 0.5;
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn test_get_three_quarter_way_low_to_normal() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(-0.25);
        let expected = 1.0 + (4.5 - 1.0) * 0.75;
        assert!((result - expected).abs() < 1e-10);
    }

    // Range: 0.0 <= level < 0.5 (normal -> medium interpolation)
    #[test]
    fn test_get_quarter_way_normal_to_medium() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(0.125);
        let expected = 4.5 + (7.0 - 4.5) * 0.25;
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn test_get_midway_normal_to_medium() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(0.25);
        let expected = 4.5 + (7.0 - 4.5) * 0.5;
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn test_get_three_quarter_way_normal_to_medium() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(0.375);
        let expected = 4.5 + (7.0 - 4.5) * 0.75;
        assert!((result - expected).abs() < 1e-10);
    }

    // Range: 0.5 <= level < 1.0 (medium -> high interpolation)
    #[test]
    fn test_get_quarter_way_medium_to_high() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(0.625);
        let expected = 7.0 + (11.0 - 7.0) * 0.25;
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn test_get_midway_medium_to_high() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(0.75);
        let expected = 7.0 + (11.0 - 7.0) * 0.5;
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn test_get_three_quarter_way_medium_to_high() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(0.875);
        let expected = 7.0 + (11.0 - 7.0) * 0.75;
        assert!((result - expected).abs() < 1e-10);
    }

    // Above 1.0
    #[test]
    fn test_get_above_high_boundary() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(2.0);
        assert!((result - 11.0).abs() < 1e-10);
    }

    #[test]
    fn test_get_far_above_high_boundary() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let result = curve.get(100.0);
        assert!((result - 11.0).abs() < 1e-10);
    }

    // Edge cases with identical values
    #[test]
    fn test_get_all_same_values() {
        let curve = ContrastCurve::new(5.0, 5.0, 5.0, 5.0);
        assert!((curve.get(-1.0) - 5.0).abs() < 1e-10);
        assert!((curve.get(-0.5) - 5.0).abs() < 1e-10);
        assert!((curve.get(0.0) - 5.0).abs() < 1e-10);
        assert!((curve.get(0.25) - 5.0).abs() < 1e-10);
        assert!((curve.get(0.5) - 5.0).abs() < 1e-10);
        assert!((curve.get(0.75) - 5.0).abs() < 1e-10);
        assert!((curve.get(1.0) - 5.0).abs() < 1e-10);
    }

    // Realistic contrast ratio values
    #[test]
    fn test_get_with_realistic_wcag_values() {
        // Typical WCAG AA contrast ratios for different levels
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 21.0);
        assert!((curve.get(-1.0) - 1.0).abs() < 1e-10);
        assert!((curve.get(0.0) - 4.5).abs() < 1e-10);
        assert!((curve.get(0.5) - 7.0).abs() < 1e-10);
        assert!((curve.get(1.0) - 21.0).abs() < 1e-10);
    }

    #[test]
    fn test_get_zero_values() {
        let curve = ContrastCurve::new(0.0, 0.0, 0.0, 0.0);
        assert!((curve.get(-1.0) - 0.0).abs() < 1e-10);
        assert!((curve.get(0.5) - 0.0).abs() < 1e-10);
        assert!((curve.get(1.0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_get_negative_values() {
        let curve = ContrastCurve::new(-5.0, -2.0, 1.0, 4.0);
        assert!((curve.get(-1.0) - (-5.0)).abs() < 1e-10);
        assert!((curve.get(0.0) - (-2.0)).abs() < 1e-10);
        assert!((curve.get(0.5) - 1.0).abs() < 1e-10);
        assert!((curve.get(1.0) - 4.0).abs() < 1e-10);
    }

    #[test]
    fn test_get_very_small_increments() {
        let curve = ContrastCurve::new(1.0, 4.5, 7.0, 11.0);
        let mut prev = curve.get(-1.0);
        // Test that values increase monotonically in this range
        for i in 0..100 {
            let level = -1.0 + (i as f64) * 0.02;
            let val = curve.get(level);
            assert!(
                val >= prev - 1e-10,
                "Value should be monotonic at level {}",
                level
            );
            prev = val;
        }
    }
}

// <FILE>crates/mcu-dynamiccolor/src/contrast_curve.rs</FILE> - <DESC>Contrast curve for dynamic color selection</DESC>
// <VERS>END OF VERSION: 1.1.0</VERS>
