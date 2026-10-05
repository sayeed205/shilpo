// SPDX-License-Identifier: Apache-2.0
// Copyright 2023 Google LLC
// Copyright 2026 JAC and Contributors
//
// Adapted from Google's Material Color Utilities (commit
// 5b3618b16fdc3825e21d5679bafd144662088ea1) via the mcu-material-color Rust port.
// Changes: embedded in the Amane algorithm module and dependency imports localized.
// <FILE>crates/mcu-temperature/src/temperature_cache.rs</FILE> - <DESC>Temperature-based color analysis with lazy caching</DESC>
// <VERS>VERSION: 1.1.1</VERS>
// <WCTX>Fix audit finding F-009: eliminate expensive clones and unsafe unwraps</WCTX>
// <CLOG>Replace .unwrap() with .expect() for invariant clarity; add NaN safety to partial_cmp; avoid unnecessary HashMap clones by caching temps separately</CLOG>

use std::collections::HashMap;

use crate::material_color::hct::Hct;
use crate::material_color::utils::color::lab_from_argb;
use crate::material_color::utils::math::{sanitize_degrees_double, sanitize_degrees_int};

/// Design utilities using color temperature theory.
///
/// Analogous colors, complementary color, and cache to efficiently, lazily,
/// generate data for calculations when needed.
///
/// # Example
///
/// ```
/// use crate::material_color::hct::Hct;
/// use crate::material_color::temperature::TemperatureCache;
///
/// let input = Hct::from(30.0, 50.0, 50.0);
/// let mut cache = TemperatureCache::new(input);
///
/// // Get complementary color
/// let complement = cache.complement();
///
/// // Get analogous colors
/// let analogous = cache.analogous(5, 12);
/// ```
pub struct TemperatureCache {
    input: Hct,
    hcts_by_temp_cache: Vec<Hct>,
    hcts_by_hue_cache: Vec<Hct>,
    temps_by_hct_cache: HashMap<u32, f64>,
    input_relative_temperature_cache: f64,
    complement_cache: Option<Hct>,
}

impl TemperatureCache {
    /// Create a new TemperatureCache for the given input color.
    ///
    /// # Arguments
    ///
    /// * `input` - The input HCT color to analyze
    pub fn new(input: Hct) -> Self {
        Self {
            input,
            hcts_by_temp_cache: Vec::new(),
            hcts_by_hue_cache: Vec::new(),
            temps_by_hct_cache: HashMap::new(),
            input_relative_temperature_cache: -1.0,
            complement_cache: None,
        }
    }

    /// Returns the warmest color with the same chroma and tone as the input.
    pub fn warmest(&mut self) -> Hct {
        let hcts = self.hcts_by_temp();
        hcts[hcts.len() - 1]
    }

    /// Returns the coldest color with the same chroma and tone as the input.
    pub fn coldest(&mut self) -> Hct {
        let hcts = self.hcts_by_temp();
        hcts[0]
    }

    /// A color that complements the input color aesthetically.
    ///
    /// In art, this is usually described as being across the color wheel.
    /// History of this shows intent as a color that is just as cool-warm as the
    /// input color is warm-cool.
    pub fn complement(&mut self) -> Hct {
        if let Some(cached) = self.complement_cache {
            return cached;
        }

        let coldest = self.coldest();
        let coldest_hue = coldest.hue();
        let coldest_argb = coldest.to_int();

        let warmest = self.warmest();
        let warmest_hue = warmest.hue();
        let warmest_argb = warmest.to_int();

        let coldest_temp = *self
            .temps_by_hct()
            .get(&coldest_argb)
            .expect("invariant: temp exists for cached coldest HCT");
        let warmest_temp = *self
            .temps_by_hct()
            .get(&warmest_argb)
            .expect("invariant: temp exists for cached warmest HCT");
        let range = warmest_temp - coldest_temp;

        let input_hue = self.input.hue();
        let start_hue_is_coldest_to_warmest = Self::is_between(input_hue, coldest_hue, warmest_hue);
        let start_hue = if start_hue_is_coldest_to_warmest {
            warmest_hue
        } else {
            coldest_hue
        };
        let end_hue = if start_hue_is_coldest_to_warmest {
            coldest_hue
        } else {
            warmest_hue
        };
        let direction_of_rotation = 1.0;
        let mut smallest_error = 1000.0;

        let mut answer = self.hcts_by_hue()[input_hue.round() as usize];

        let complement_relative_temp = 1.0 - self.input_relative_temperature();

        // Find the color in the other section, closest to the inverse percentile
        // of the input color. This is the complement.
        let mut hue_addend = 0.0;
        while hue_addend <= 360.0 {
            let hue = sanitize_degrees_double(start_hue + direction_of_rotation * hue_addend);
            if !Self::is_between(hue, start_hue, end_hue) {
                hue_addend += 1.0;
                continue;
            }
            let possible_answer = self.hcts_by_hue()[hue.round() as usize];
            let possible_temp = *self
                .temps_by_hct()
                .get(&possible_answer.to_int())
                .expect("invariant: temp exists for cached HCT");
            let relative_temp = (possible_temp - coldest_temp) / range;
            let error = (complement_relative_temp - relative_temp).abs();
            if error < smallest_error {
                smallest_error = error;
                answer = possible_answer;
            }
            hue_addend += 1.0;
        }

        self.complement_cache = Some(answer);
        answer
    }

    /// A set of colors with differing hues, equidistant in temperature.
    ///
    /// In art, this is usually described as a set of 5 colors on a color wheel
    /// divided into 12 sections. This method allows provision of either of those
    /// values.
    ///
    /// Behavior is undefined when `count` or `divisions` is 0.
    /// When divisions < count, colors repeat.
    ///
    /// # Arguments
    ///
    /// * `count` - The number of colors to return, includes the input color.
    /// * `divisions` - The number of divisions on the color wheel.
    pub fn analogous(&mut self, count: i32, divisions: i32) -> Vec<Hct> {
        let start_hue = self.input.hue().round() as i32;
        let start_hct = self.hcts_by_hue()[start_hue as usize];
        let mut last_temp = self.relative_temperature(&start_hct);
        let mut all_colors = vec![start_hct];

        let mut absolute_total_temp_delta = 0.0;
        for i in 0..360 {
            let hue = sanitize_degrees_int(start_hue + i);
            let hct = self.hcts_by_hue()[hue as usize];
            let temp = self.relative_temperature(&hct);
            let temp_delta = (temp - last_temp).abs();
            last_temp = temp;
            absolute_total_temp_delta += temp_delta;
        }

        let mut hue_addend = 1;
        let temp_step = absolute_total_temp_delta / divisions as f64;
        let mut total_temp_delta = 0.0;
        last_temp = self.relative_temperature(&start_hct);

        while all_colors.len() < divisions as usize {
            let hue = sanitize_degrees_int(start_hue + hue_addend);
            let hct = self.hcts_by_hue()[hue as usize];
            let temp = self.relative_temperature(&hct);
            let temp_delta = (temp - last_temp).abs();
            total_temp_delta += temp_delta;

            let desired_total_temp_delta_for_index = all_colors.len() as f64 * temp_step;
            let mut index_satisfied = total_temp_delta >= desired_total_temp_delta_for_index;
            let mut index_addend = 1;

            // Keep adding this hue to the answers until its temperature is
            // insufficient. This ensures consistent behavior when there aren't
            // [divisions] discrete steps between 0 and 360 in hue with [tempStep]
            // delta in temperature between them.
            //
            // For example, white and black have no analogues: there are no other
            // colors at T100/T0. Therefore, they should just be added to the array
            // as answers.
            while index_satisfied && all_colors.len() < divisions as usize {
                all_colors.push(hct);
                let desired_total_temp_delta_for_index =
                    (all_colors.len() + index_addend) as f64 * temp_step;
                index_satisfied = total_temp_delta >= desired_total_temp_delta_for_index;
                index_addend += 1;
            }

            last_temp = temp;
            hue_addend += 1;

            if hue_addend > 360 {
                while all_colors.len() < divisions as usize {
                    all_colors.push(hct);
                }
                break;
            }
        }

        let mut answers = vec![self.input];

        // First, generate analogues from rotating counter-clockwise.
        let increase_hue_count = ((count - 1) as f64 / 2.0).floor() as i32;
        for i in 1..(increase_hue_count + 1) {
            let mut index = 0 - i;
            while index < 0 {
                index += all_colors.len() as i32;
            }
            if index >= all_colors.len() as i32 {
                index %= all_colors.len() as i32;
            }
            answers.insert(0, all_colors[index as usize]);
        }

        // Second, generate analogues from rotating clockwise.
        let decrease_hue_count = count - increase_hue_count - 1;
        for i in 1..(decrease_hue_count + 1) {
            let mut index = i;
            while index < 0 {
                index += all_colors.len() as i32;
            }
            if index >= all_colors.len() as i32 {
                index %= all_colors.len() as i32;
            }
            answers.push(all_colors[index as usize]);
        }

        answers
    }

    /// Temperature relative to all colors with the same chroma and tone.
    /// Value on a scale from 0 to 1.
    pub fn relative_temperature(&mut self, hct: &Hct) -> f64 {
        let warmest_argb = self.warmest().to_int();
        let coldest_argb = self.coldest().to_int();

        let warmest_temp = *self
            .temps_by_hct()
            .get(&warmest_argb)
            .expect("invariant: temp exists for cached warmest HCT");
        let coldest_temp = *self
            .temps_by_hct()
            .get(&coldest_argb)
            .expect("invariant: temp exists for cached coldest HCT");
        let range = warmest_temp - coldest_temp;
        let difference_from_coldest = *self
            .temps_by_hct()
            .get(&hct.to_int())
            .expect("invariant: temp exists for cached HCT")
            - coldest_temp;

        // Handle when there's no difference in temperature between warmest and
        // coldest: for example, at T100, only one color is available, white.
        if range == 0.0 {
            return 0.5;
        }

        difference_from_coldest / range
    }

    /// Relative temperature of the input color. See `relative_temperature`.
    fn input_relative_temperature(&mut self) -> f64 {
        if self.input_relative_temperature_cache >= 0.0 {
            return self.input_relative_temperature_cache;
        }

        let input = self.input;
        self.input_relative_temperature_cache = self.relative_temperature(&input);
        self.input_relative_temperature_cache
    }

    /// A map with keys of HCT ARGB values, values of raw temperature.
    fn temps_by_hct(&mut self) -> &HashMap<u32, f64> {
        if !self.temps_by_hct_cache.is_empty() {
            return &self.temps_by_hct_cache;
        }

        let hcts_by_hue = self.hcts_by_hue().to_vec();
        let input = self.input;
        let mut all_hcts = hcts_by_hue;
        all_hcts.push(input);

        let mut temps_by_hct = HashMap::new();
        for hct in &all_hcts {
            temps_by_hct.insert(hct.to_int(), Self::raw_temperature(hct));
        }

        self.temps_by_hct_cache = temps_by_hct;
        &self.temps_by_hct_cache
    }

    /// HCTs for all hues, with the same chroma/tone as the input.
    /// Sorted ascending, hue 0 to 360.
    fn hcts_by_hue(&mut self) -> &Vec<Hct> {
        if !self.hcts_by_hue_cache.is_empty() {
            return &self.hcts_by_hue_cache;
        }

        let mut hcts = Vec::new();
        let mut hue = 0.0;
        while hue <= 360.0 {
            let color_at_hue = Hct::from(hue, self.input.chroma(), self.input.tone());
            hcts.push(color_at_hue);
            hue += 1.0;
        }

        self.hcts_by_hue_cache = hcts;
        &self.hcts_by_hue_cache
    }

    /// HCTs sorted by temperature, ascending (coldest to warmest).
    fn hcts_by_temp(&mut self) -> Vec<Hct> {
        if !self.hcts_by_temp_cache.is_empty() {
            return self.hcts_by_temp_cache.clone();
        }

        let hcts_by_hue = self.hcts_by_hue().to_vec();
        let input = self.input;
        let mut hcts = hcts_by_hue;
        hcts.push(input);

        // Ensure temperature cache is populated before sorting
        let _ = self.temps_by_hct();

        // Pair each HCT with its temperature to avoid cloning the entire HashMap
        let mut hcts_with_temp: Vec<(Hct, f64)> = hcts
            .into_iter()
            .map(|hct| {
                let temp = *self
                    .temps_by_hct_cache
                    .get(&hct.to_int())
                    .expect("invariant: temp exists for cached HCT");
                (hct, temp)
            })
            .collect();

        hcts_with_temp.sort_by(|a, b| a.1.total_cmp(&b.1));

        self.hcts_by_temp_cache = hcts_with_temp.into_iter().map(|(hct, _)| hct).collect();
        self.hcts_by_temp_cache.clone()
    }

    /// Determines if an angle is between two other angles, rotating clockwise.
    fn is_between(angle: f64, a: f64, b: f64) -> bool {
        if a < b {
            a <= angle && angle <= b
        } else {
            a <= angle || angle <= b
        }
    }

    /// Value representing cool-warm factor of a color.
    /// Values below 0 are considered cool, above, warm.
    ///
    /// Color science has researched emotion and harmony, which art uses to select
    /// colors. Warm-cool is the foundation of analogous and complementary colors.
    /// See:
    /// - Li-Chen Ou's Chapter 19 in Handbook of Color Psychology (2015).
    /// - Josef Albers' Interaction of Color chapters 19 and 21.
    ///
    /// Implementation of Ou, Woodcock and Wright's algorithm, which uses
    /// L*a*b* / LCH color space.
    /// Return value has these properties:
    /// - Values below 0 are cool, above 0 are warm.
    /// - Lower bound: -0.52 - (chroma ^ 1.07 / 20). L*a*b* chroma is infinite.
    ///   Assuming max of 130 chroma, -9.66.
    /// - Upper bound: -0.52 + (chroma ^ 1.07 / 20). L*a*b* chroma is infinite.
    ///   Assuming max of 130 chroma, 8.61.
    pub fn raw_temperature(color: &Hct) -> f64 {
        let lab = lab_from_argb(color.to_int());
        let hue = sanitize_degrees_double(lab[2].atan2(lab[1]) * 180.0 / std::f64::consts::PI);
        let chroma = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
        -0.5 + 0.02
            * chroma.powf(1.07)
            * (sanitize_degrees_double(hue - 50.0) * std::f64::consts::PI / 180.0).cos()
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

    // ==================== Construction Tests ====================

    #[test]
    fn test_new_creates_cache() {
        let hct = Hct::from(180.0, 50.0, 50.0);
        let cache = TemperatureCache::new(hct);
        assert_eq!(cache.input.to_int(), hct.to_int());
    }

    // ==================== Raw Temperature Tests ====================

    #[test]
    fn test_raw_temperature_returns_value() {
        let hct = Hct::from(0.0, 50.0, 50.0);
        let temp = TemperatureCache::raw_temperature(&hct);
        // Temperature should be within expected bounds
        assert!(temp > -10.0 && temp < 10.0);
    }

    #[test]
    fn test_raw_temperature_warm_colors() {
        // Red-orange hues should be warm (positive temperature)
        let warm_hct = Hct::from(30.0, 80.0, 50.0);
        let temp = TemperatureCache::raw_temperature(&warm_hct);
        assert!(
            temp > 0.0,
            "Expected positive temperature for warm color, got {}",
            temp
        );
    }

    #[test]
    fn test_raw_temperature_cool_colors() {
        // Blue-cyan hues should be cool (negative temperature)
        let cool_hct = Hct::from(200.0, 80.0, 50.0);
        let temp = TemperatureCache::raw_temperature(&cool_hct);
        assert!(
            temp < 0.0,
            "Expected negative temperature for cool color, got {}",
            temp
        );
    }

    // ==================== Warmest/Coldest Tests ====================

    #[test]
    fn test_warmest_returns_hct() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);
        let warmest = cache.warmest();
        // Should return a valid HCT
        assert!(warmest.hue() >= 0.0 && warmest.hue() < 360.0);
    }

    #[test]
    fn test_coldest_returns_hct() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);
        let coldest = cache.coldest();
        // Should return a valid HCT
        assert!(coldest.hue() >= 0.0 && coldest.hue() < 360.0);
    }

    #[test]
    fn test_warmest_warmer_than_coldest() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);
        let warmest = cache.warmest();
        let coldest = cache.coldest();
        let warmest_temp = TemperatureCache::raw_temperature(&warmest);
        let coldest_temp = TemperatureCache::raw_temperature(&coldest);
        assert!(
            warmest_temp >= coldest_temp,
            "Warmest ({}) should be >= coldest ({})",
            warmest_temp,
            coldest_temp
        );
    }

    // ==================== Complement Tests ====================

    #[test]
    fn test_complement_returns_hct() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);
        let complement = cache.complement();
        // Should return a valid HCT
        assert!(complement.hue() >= 0.0 && complement.hue() < 360.0);
    }

    #[test]
    fn test_complement_different_from_input() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);
        let complement = cache.complement();
        // Complement should have different hue (unless achromatic)
        // The complement is not simply 180 degrees away, but opposite in temperature
        assert!(complement.to_int() != hct.to_int() || hct.chroma() < 1.0);
    }

    #[test]
    fn test_complement_caching() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);
        let complement1 = cache.complement();
        let complement2 = cache.complement();
        assert_eq!(complement1.to_int(), complement2.to_int());
    }

    // ==================== Analogous Tests ====================

    #[test]
    fn test_analogous_returns_correct_count() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);
        let analogous = cache.analogous(5, 12);
        assert_eq!(analogous.len(), 5);
    }

    #[test]
    fn test_analogous_includes_input() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);
        let analogous = cache.analogous(5, 12);
        // The input should be in the middle of the result
        let input_argb = hct.to_int();
        assert!(
            analogous.iter().any(|h| h.to_int() == input_argb),
            "Input should be included in analogous colors"
        );
    }

    #[test]
    fn test_analogous_with_different_counts() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);

        let analogous_3 = cache.analogous(3, 12);
        assert_eq!(analogous_3.len(), 3);

        let analogous_7 = cache.analogous(7, 12);
        assert_eq!(analogous_7.len(), 7);
    }

    #[test]
    fn test_analogous_with_different_divisions() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);

        let analogous_6 = cache.analogous(5, 6);
        assert_eq!(analogous_6.len(), 5);

        let analogous_24 = cache.analogous(5, 24);
        assert_eq!(analogous_24.len(), 5);
    }

    // ==================== Relative Temperature Tests ====================

    #[test]
    fn test_relative_temperature_bounds() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);

        let warmest = cache.warmest();
        let coldest = cache.coldest();

        let warmest_rel = cache.relative_temperature(&warmest);
        let coldest_rel = cache.relative_temperature(&coldest);

        assert_relative_eq!(warmest_rel, 1.0, epsilon = 0.001);
        assert_relative_eq!(coldest_rel, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_relative_temperature_range() {
        let hct = Hct::from(180.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);
        let rel_temp = cache.relative_temperature(&hct);
        assert!((0.0..=1.0).contains(&rel_temp));
    }

    // ==================== IsBetween Tests ====================

    #[test]
    fn test_is_between_simple_range() {
        assert!(TemperatureCache::is_between(45.0, 0.0, 90.0));
        assert!(!TemperatureCache::is_between(100.0, 0.0, 90.0));
    }

    #[test]
    fn test_is_between_wrapping_range() {
        // When a > b, the range wraps around
        assert!(TemperatureCache::is_between(350.0, 300.0, 30.0));
        assert!(TemperatureCache::is_between(10.0, 300.0, 30.0));
        assert!(!TemperatureCache::is_between(180.0, 300.0, 30.0));
    }

    #[test]
    fn test_is_between_boundary() {
        assert!(TemperatureCache::is_between(0.0, 0.0, 90.0));
        assert!(TemperatureCache::is_between(90.0, 0.0, 90.0));
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_achromatic_color() {
        // White - no chroma
        let white = Hct::from(0.0, 0.0, 100.0);
        let mut cache = TemperatureCache::new(white);
        let analogous = cache.analogous(5, 12);
        assert_eq!(analogous.len(), 5);
    }

    #[test]
    fn test_black_color() {
        // Black - no chroma
        let black = Hct::from(0.0, 0.0, 0.0);
        let mut cache = TemperatureCache::new(black);
        let analogous = cache.analogous(5, 12);
        assert_eq!(analogous.len(), 5);
    }

    #[test]
    fn test_gray_relative_temperature() {
        // Gray has low/no chroma, so temperature range is small
        // The relative temperature should still be in [0, 1] range
        let gray = Hct::from(0.0, 0.0, 50.0);
        let mut cache = TemperatureCache::new(gray);
        let rel_temp = cache.relative_temperature(&gray);
        // Relative temperature should be in valid range
        assert!(
            (0.0..=1.0).contains(&rel_temp),
            "Relative temperature {} should be in [0, 1]",
            rel_temp
        );
    }

    // ==================== Specific Hue Tests ====================

    #[test]
    fn test_blue_is_cool() {
        let blue = Hct::from(240.0, 80.0, 50.0);
        let temp = TemperatureCache::raw_temperature(&blue);
        assert!(
            temp < 0.0,
            "Blue should be cool (negative temperature), got {}",
            temp
        );
    }

    #[test]
    fn test_orange_is_warm() {
        let orange = Hct::from(30.0, 80.0, 60.0);
        let temp = TemperatureCache::raw_temperature(&orange);
        assert!(
            temp > 0.0,
            "Orange should be warm (positive temperature), got {}",
            temp
        );
    }

    #[test]
    fn test_green_temperature() {
        let green = Hct::from(120.0, 50.0, 50.0);
        let temp = TemperatureCache::raw_temperature(&green);
        // Green is neither strongly warm nor cool
        assert!(
            temp.abs() < 5.0,
            "Green should be near neutral, got {}",
            temp
        );
    }

    // ==================== Consistency Tests ====================

    #[test]
    fn test_multiple_calls_same_result() {
        let hct = Hct::from(30.0, 50.0, 50.0);
        let mut cache = TemperatureCache::new(hct);

        let warmest1 = cache.warmest();
        let warmest2 = cache.warmest();
        assert_eq!(warmest1.to_int(), warmest2.to_int());

        let coldest1 = cache.coldest();
        let coldest2 = cache.coldest();
        assert_eq!(coldest1.to_int(), coldest2.to_int());
    }
}

// <FILE>crates/mcu-temperature/src/temperature_cache.rs</FILE> - <DESC>Temperature-based color analysis with lazy caching</DESC>
// <VERS>END OF VERSION: 1.1.1</VERS>
