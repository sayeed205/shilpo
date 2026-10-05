// SPDX-License-Identifier: Apache-2.0
// Copyright 2021 Google LLC
// Copyright 2026 JAC and Contributors
//
// Adapted from Google's Material Color Utilities (commit
// 5b3618b16fdc3825e21d5679bafd144662088ea1) via the mcu-material-color Rust port.
// Changes: embedded in the Amane algorithm module and defaults are computed locally.
// <FILE>crates/mcu-hct/src/viewing_conditions.rs</FILE> - <DESC>ViewingConditions for CAM16 color appearance model</DESC>
// <VERS>VERSION: 1.0.0</VERS>
// <WCTX>Port ViewingConditions from TypeScript Material Color Utilities</WCTX>
// <CLOG>Implement ViewingConditions struct with make() constructor and DEFAULT constant</CLOG>

use crate::material_color::utils::color::{white_point_d65, y_from_lstar};
use crate::material_color::utils::math::lerp;

/// Viewing conditions for the CAM16 color appearance model.
///
/// In traditional color spaces, a color can be identified solely by the
/// observer's measurement of the color. Color appearance models such as CAM16
/// also use information about the environment where the color was observed,
/// known as the viewing conditions.
///
/// For example, white under the traditional assumption of a midday sun white
/// point is accurately measured as a slightly chromatic blue by CAM16.
/// (roughly, hue 203, chroma 3, lightness 100)
///
/// This struct caches intermediate values of the CAM16 conversion process that
/// depend only on viewing conditions, enabling speed ups.
#[derive(Debug, Clone)]
pub struct ViewingConditions {
    /// Background luminance factor (Y_background / Y_white)
    pub n: f64,
    /// Achromatic response to white
    pub aw: f64,
    /// Background brightness factor
    pub nbb: f64,
    /// Chromatic brightness factor
    pub ncb: f64,
    /// Surround impact on chroma
    pub c: f64,
    /// Surround chromatic induction factor
    pub nc: f64,
    /// Chromatic adaptation factors for RGB
    pub rgb_d: [f64; 3],
    /// Luminance adaptation factor
    pub fl: f64,
    /// Fourth root of luminance adaptation factor
    pub fl_root: f64,
    /// Base exponential nonlinearity
    pub z: f64,

    // Input parameters (stored for testing/debugging)
    adapting_luminance: f64,
    background_lstar: f64,
    surround: f64,
    discounting_illuminant: bool,
}

impl ViewingConditions {
    /// Create ViewingConditions from a simple, physically relevant, set of parameters.
    ///
    /// # Arguments
    ///
    /// * `white_point` - White point, measured in the XYZ color space.
    ///   Default = D65, or sunny day afternoon.
    /// * `adapting_luminance` - The luminance of the adapting field. Informally,
    ///   how bright it is in the room where the color is viewed. Can be
    ///   calculated from lux by multiplying lux by 0.0586. Default = 11.72,
    ///   or 200 lux.
    /// * `background_lstar` - The lightness of the area surrounding the color,
    ///   measured by L* in L*a*b*. Default = 50.0
    /// * `surround` - A general description of the lighting surrounding the
    ///   color. 0 is pitch dark, like watching a movie in a theater. 1.0 is a
    ///   dimly lit room, like watching TV at home at night. 2.0 means there
    ///   is no difference between the lighting on the color and around it.
    ///   Default = 2.0
    /// * `discounting_illuminant` - Whether the eye accounts for the tint of the
    ///   ambient lighting, such as knowing an apple is still red in green light.
    ///   Default = false, the eye does not perform this process on
    ///   self-luminous objects like displays.
    pub fn make(
        white_point: [f64; 3],
        adapting_luminance: f64,
        background_lstar: f64,
        surround: f64,
        discounting_illuminant: bool,
    ) -> Self {
        let xyz = white_point;

        // XYZ to Hunt-Pointer-Estevez cone responses (M16 matrix)
        let r_w = xyz[0] * 0.401288 + xyz[1] * 0.650173 + xyz[2] * -0.051461;
        let g_w = xyz[0] * -0.250268 + xyz[1] * 1.204414 + xyz[2] * 0.045854;
        let b_w = xyz[0] * -0.002079 + xyz[1] * 0.048952 + xyz[2] * 0.953127;

        // Surround parameters
        let f = 0.8 + surround / 10.0;
        let c = if f >= 0.9 {
            lerp(0.59, 0.69, (f - 0.9) * 10.0)
        } else {
            lerp(0.525, 0.59, (f - 0.8) * 10.0)
        };

        // Degree of adaptation
        let mut d = if discounting_illuminant {
            1.0
        } else {
            f * (1.0 - (1.0 / 3.6) * ((-adapting_luminance - 42.0) / 92.0).exp())
        };
        d = d.clamp(0.0, 1.0);

        let nc = f;

        // Chromatic adaptation
        let rgb_d = [
            d * (100.0 / r_w) + 1.0 - d,
            d * (100.0 / g_w) + 1.0 - d,
            d * (100.0 / b_w) + 1.0 - d,
        ];

        // Luminance adaptation
        let k = 1.0 / (5.0 * adapting_luminance + 1.0);
        let k4 = k * k * k * k;
        let k4f = 1.0 - k4;
        let fl = k4 * adapting_luminance + 0.1 * k4f * k4f * (5.0 * adapting_luminance).cbrt();

        // Background influence
        let n = y_from_lstar(background_lstar) / white_point[1];
        let z = 1.48 + n.sqrt();
        let nbb = 0.725 / n.powf(0.2);
        let ncb = nbb;

        // Achromatic response to white
        let rgb_a_factors = [
            ((fl * rgb_d[0] * r_w) / 100.0).powf(0.42),
            ((fl * rgb_d[1] * g_w) / 100.0).powf(0.42),
            ((fl * rgb_d[2] * b_w) / 100.0).powf(0.42),
        ];
        let rgb_a = [
            (400.0 * rgb_a_factors[0]) / (rgb_a_factors[0] + 27.13),
            (400.0 * rgb_a_factors[1]) / (rgb_a_factors[1] + 27.13),
            (400.0 * rgb_a_factors[2]) / (rgb_a_factors[2] + 27.13),
        ];
        let aw = (2.0 * rgb_a[0] + rgb_a[1] + 0.05 * rgb_a[2]) * nbb;

        Self {
            n,
            aw,
            nbb,
            ncb,
            c,
            nc,
            rgb_d,
            fl,
            fl_root: fl.powf(0.25),
            z,
            adapting_luminance,
            background_lstar,
            surround,
            discounting_illuminant,
        }
    }

    /// Create default viewing conditions with standard sRGB parameters.
    fn default_make() -> Self {
        let white_point = white_point_d65();
        let adapting_luminance = (200.0 / std::f64::consts::PI) * y_from_lstar(50.0) / 100.0;
        Self::make(white_point, adapting_luminance, 50.0, 2.0, false)
    }

    /// Get the adapting luminance used to create these viewing conditions.
    pub fn adapting_luminance(&self) -> f64 {
        self.adapting_luminance
    }

    /// Get the background L* used to create these viewing conditions.
    pub fn background_lstar(&self) -> f64 {
        self.background_lstar
    }

    /// Get the surround parameter used to create these viewing conditions.
    pub fn surround(&self) -> f64 {
        self.surround
    }

    /// Get whether discounting illuminant was enabled.
    pub fn discounting_illuminant(&self) -> bool {
        self.discounting_illuminant
    }
}

impl Default for ViewingConditions {
    fn default() -> Self {
        Self::default_make()
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

    #[test]
    fn test_default_adapting_luminance() {
        let vc = ViewingConditions::default();
        assert_relative_eq!(vc.adapting_luminance(), 11.725677948856951, epsilon = 1e-10);
    }

    #[test]
    fn test_default_background_lstar() {
        let vc = ViewingConditions::default();
        assert_relative_eq!(vc.background_lstar(), 50.0, epsilon = 1e-10);
    }

    #[test]
    fn test_default_surround() {
        let vc = ViewingConditions::default();
        assert_relative_eq!(vc.surround(), 2.0, epsilon = 1e-10);
    }

    #[test]
    fn test_default_discounting_illuminant() {
        let vc = ViewingConditions::default();
        assert!(!vc.discounting_illuminant());
    }

    #[test]
    fn test_make_custom_background_lstar() {
        let white_point = white_point_d65();
        let adapting_luminance = (200.0 / std::f64::consts::PI) * y_from_lstar(50.0) / 100.0;
        let vc = ViewingConditions::make(white_point, adapting_luminance, 30.0, 2.0, false);
        assert_relative_eq!(vc.background_lstar(), 30.0, epsilon = 1e-10);
    }

    #[test]
    fn test_make_custom_surround() {
        let white_point = white_point_d65();
        let adapting_luminance = (200.0 / std::f64::consts::PI) * y_from_lstar(50.0) / 100.0;
        let vc = ViewingConditions::make(white_point, adapting_luminance, 50.0, 1.0, false);
        assert_relative_eq!(vc.surround(), 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_make_discounting_illuminant() {
        let white_point = white_point_d65();
        let adapting_luminance = (200.0 / std::f64::consts::PI) * y_from_lstar(50.0) / 100.0;
        let vc = ViewingConditions::make(white_point, adapting_luminance, 50.0, 2.0, true);
        assert!(vc.discounting_illuminant());
    }

    #[test]
    fn test_n_calculation() {
        let vc = ViewingConditions::default();
        // n = Y_background / Y_white
        // Y_background = y_from_lstar(50.0) ≈ 18.418651851244416
        // Y_white = white_point_d65()[1] = 100.0
        // n ≈ 0.18418651851244416
        assert_relative_eq!(vc.n, 0.18418651851244416, epsilon = 1e-10);
    }

    #[test]
    fn test_z_calculation() {
        let vc = ViewingConditions::default();
        // z = 1.48 + sqrt(n)
        let expected_z = 1.48 + 0.18418651851244416_f64.sqrt();
        assert_relative_eq!(vc.z, expected_z, epsilon = 1e-10);
    }

    #[test]
    fn test_nbb_ncb_calculation() {
        let vc = ViewingConditions::default();
        // nbb = 0.725 / n^0.2
        let n = 0.18418651851244416_f64;
        let expected_nbb = 0.725 / n.powf(0.2);
        assert_relative_eq!(vc.nbb, expected_nbb, epsilon = 1e-10);
        assert_relative_eq!(vc.ncb, expected_nbb, epsilon = 1e-10);
    }

    #[test]
    fn test_c_for_high_surround() {
        // f = 0.8 + 2.0/10.0 = 1.0, f >= 0.9
        // c = lerp(0.59, 0.69, (1.0 - 0.9) * 10) = lerp(0.59, 0.69, 1.0) = 0.69
        let vc = ViewingConditions::default();
        assert_relative_eq!(vc.c, 0.69, epsilon = 1e-10);
    }

    #[test]
    fn test_c_for_low_surround() {
        let white_point = white_point_d65();
        let adapting_luminance = (200.0 / std::f64::consts::PI) * y_from_lstar(50.0) / 100.0;
        // surround = 0.5, f = 0.8 + 0.05 = 0.85, f < 0.9
        // c = lerp(0.525, 0.59, (0.85 - 0.8) * 10) = lerp(0.525, 0.59, 0.5) = 0.5575
        let vc = ViewingConditions::make(white_point, adapting_luminance, 50.0, 0.5, false);
        assert_relative_eq!(vc.c, 0.5575, epsilon = 1e-10);
    }

    #[test]
    fn test_nc_equals_f() {
        let vc = ViewingConditions::default();
        // nc = f = 0.8 + surround/10 = 0.8 + 0.2 = 1.0
        assert_relative_eq!(vc.nc, 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_rgb_d_length() {
        let vc = ViewingConditions::default();
        assert_eq!(vc.rgb_d.len(), 3);
    }

    #[test]
    fn test_fl_root_calculation() {
        let vc = ViewingConditions::default();
        assert_relative_eq!(vc.fl_root, vc.fl.powf(0.25), epsilon = 1e-10);
    }

    #[test]
    fn test_aw_positive() {
        let vc = ViewingConditions::default();
        assert!(vc.aw > 0.0);
    }

    #[test]
    fn test_fl_positive() {
        let vc = ViewingConditions::default();
        assert!(vc.fl > 0.0);
    }

    #[test]
    fn test_clone() {
        let vc1 = ViewingConditions::default();
        let vc2 = vc1.clone();
        assert_relative_eq!(vc1.aw, vc2.aw, epsilon = 1e-10);
        assert_relative_eq!(vc1.n, vc2.n, epsilon = 1e-10);
    }

    #[test]
    fn test_debug_impl() {
        let vc = ViewingConditions::default();
        let debug_str = format!("{:?}", vc);
        assert!(debug_str.contains("ViewingConditions"));
    }
}

// <FILE>crates/mcu-hct/src/viewing_conditions.rs</FILE> - <DESC>ViewingConditions for CAM16 color appearance model</DESC>
// <VERS>END OF VERSION: 1.0.0</VERS>
