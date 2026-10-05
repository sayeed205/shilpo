// SPDX-License-Identifier: Apache-2.0
// Copyright 2021 Google LLC
// Copyright 2026 JAC and Contributors
//
// Adapted from Google's Material Color Utilities (commit
// 5b3618b16fdc3825e21d5679bafd144662088ea1) via the mcu-material-color Rust port.
// Changes: embedded in the Amane algorithm module and dependency imports localized.
// <FILE>crates/mcu-hct/src/cam16.rs</FILE> - <DESC>CAM16 color appearance model implementation</DESC>
// <VERS>VERSION: 1.0.0</VERS>
// <WCTX>Port CAM16 from TypeScript Material Color Utilities</WCTX>
// <CLOG>Implement Cam16 struct with all conversion methods</CLOG>

use super::viewing_conditions::ViewingConditions;
use crate::material_color::utils::color::{argb_from_xyz, linearized};
use crate::material_color::utils::math::{sanitize_degrees_double, signum};

/// CAM16, a color appearance model.
///
/// Colors are not just defined by their hex code, but rather, a hex code and viewing conditions.
///
/// CAM16 instances also have coordinates in the CAM16-UCS space, called J*, a*, b*,
/// or jstar, astar, bstar in code. CAM16-UCS is included in the CAM16 specification,
/// and should be used when measuring distances between colors.
///
/// In traditional color spaces, a color can be identified solely by the observer's
/// measurement of the color. Color appearance models such as CAM16 also use information
/// about the environment where the color was observed, known as the viewing conditions.
///
/// For example, white under the traditional assumption of a midday sun white point
/// is accurately measured as a slightly chromatic blue by CAM16.
/// (roughly, hue 203, chroma 3, lightness 100)
#[derive(Debug, Clone, Copy)]
pub struct Cam16 {
    /// CAM16 hue in degrees [0, 360)
    pub hue: f64,
    /// CAM16 chroma (colorfulness / color intensity, like saturation but perceptually accurate)
    pub chroma: f64,
    /// CAM16 lightness J
    pub j: f64,
    /// CAM16 brightness Q (ratio of lightness to white point's lightness)
    pub q: f64,
    /// CAM16 colorfulness M
    pub m: f64,
    /// CAM16 saturation s (ratio of chroma to white point's chroma)
    pub s: f64,
    /// CAM16-UCS J* coordinate
    pub jstar: f64,
    /// CAM16-UCS a* coordinate
    pub astar: f64,
    /// CAM16-UCS b* coordinate
    pub bstar: f64,
}

impl Cam16 {
    /// Create a new Cam16 instance with all dimensions.
    ///
    /// Prefer using static methods that construct from 3 dimensions.
    /// This constructor is intended for those methods to use to return all possible dimensions.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hue: f64,
        chroma: f64,
        j: f64,
        q: f64,
        m: f64,
        s: f64,
        jstar: f64,
        astar: f64,
        bstar: f64,
    ) -> Self {
        Self {
            hue,
            chroma,
            j,
            q,
            m,
            s,
            jstar,
            astar,
            bstar,
        }
    }

    /// Calculate the distance between two colors in CAM16-UCS space.
    ///
    /// CAM16-UCS is included in the CAM16 specification and is used to measure
    /// distances between colors.
    pub fn distance(&self, other: &Cam16) -> f64 {
        let d_j = self.jstar - other.jstar;
        let d_a = self.astar - other.astar;
        let d_b = self.bstar - other.bstar;
        let d_e_prime = (d_j * d_j + d_a * d_a + d_b * d_b).sqrt();
        1.41 * d_e_prime.powf(0.63)
    }

    /// Create CAM16 from an ARGB color, assuming default viewing conditions.
    pub fn from_int(argb: u32) -> Self {
        Self::from_int_in_viewing_conditions(argb, &ViewingConditions::default())
    }

    /// Create CAM16 from an ARGB color with specific viewing conditions.
    pub fn from_int_in_viewing_conditions(argb: u32, vc: &ViewingConditions) -> Self {
        let red = ((argb >> 16) & 0xFF) as u8;
        let green = ((argb >> 8) & 0xFF) as u8;
        let blue = (argb & 0xFF) as u8;

        let red_l = linearized(red);
        let green_l = linearized(green);
        let blue_l = linearized(blue);

        let x = 0.41233895 * red_l + 0.35762064 * green_l + 0.18051042 * blue_l;
        let y = 0.2126 * red_l + 0.7152 * green_l + 0.0722 * blue_l;
        let z = 0.01932141 * red_l + 0.11916382 * green_l + 0.95034478 * blue_l;

        Self::from_xyz_in_viewing_conditions(x, y, z, vc)
    }

    /// Create CAM16 from XYZ coordinates with specific viewing conditions.
    pub fn from_xyz_in_viewing_conditions(x: f64, y: f64, z: f64, vc: &ViewingConditions) -> Self {
        // Transform XYZ to cone responses (M16 matrix)
        let r_c = 0.401288 * x + 0.650173 * y - 0.051461 * z;
        let g_c = -0.250268 * x + 1.204414 * y + 0.045854 * z;
        let b_c = -0.002079 * x + 0.048952 * y + 0.953127 * z;

        // Discount illuminant
        let r_d = vc.rgb_d[0] * r_c;
        let g_d = vc.rgb_d[1] * g_c;
        let b_d = vc.rgb_d[2] * b_c;

        // Chromatic adaptation
        let r_af = ((vc.fl * r_d.abs()) / 100.0).powf(0.42);
        let g_af = ((vc.fl * g_d.abs()) / 100.0).powf(0.42);
        let b_af = ((vc.fl * b_d.abs()) / 100.0).powf(0.42);

        let r_a = (signum(r_d) as f64) * 400.0 * r_af / (r_af + 27.13);
        let g_a = (signum(g_d) as f64) * 400.0 * g_af / (g_af + 27.13);
        let b_a = (signum(b_d) as f64) * 400.0 * b_af / (b_af + 27.13);

        // Redness-greenness
        let a = (11.0 * r_a + -12.0 * g_a + b_a) / 11.0;
        // Yellowness-blueness
        let b = (r_a + g_a - 2.0 * b_a) / 9.0;

        // Auxiliary components
        let u = (20.0 * r_a + 20.0 * g_a + 21.0 * b_a) / 20.0;
        let p2 = (40.0 * r_a + 20.0 * g_a + b_a) / 20.0;

        // Hue
        let atan2 = b.atan2(a);
        let atan_degrees = atan2 * 180.0 / std::f64::consts::PI;
        let hue = sanitize_degrees_double(atan_degrees);
        let hue_radians = hue * std::f64::consts::PI / 180.0;

        // Achromatic response to color
        let ac = p2 * vc.nbb;

        // CAM16 lightness and brightness
        let j = 100.0 * (ac / vc.aw).powf(vc.c * vc.z);
        let q = (4.0 / vc.c) * (j / 100.0).sqrt() * (vc.aw + 4.0) * vc.fl_root;

        let hue_prime = if hue < 20.14 { hue + 360.0 } else { hue };
        let e_hue = 0.25 * ((hue_prime * std::f64::consts::PI / 180.0 + 2.0).cos() + 3.8);
        let p1 = (50000.0 / 13.0) * e_hue * vc.nc * vc.ncb;
        let t = (p1 * (a * a + b * b).sqrt()) / (u + 0.305);
        let alpha = t.powf(0.9) * (1.64 - 0.29_f64.powf(vc.n)).powf(0.73);

        // CAM16 chroma, colorfulness, saturation
        let chroma = alpha * (j / 100.0).sqrt();
        let m = chroma * vc.fl_root;
        let s = 50.0 * ((alpha * vc.c) / (vc.aw + 4.0)).sqrt();

        // CAM16-UCS components
        let jstar = ((1.0 + 100.0 * 0.007) * j) / (1.0 + 0.007 * j);
        let mstar = (1.0 / 0.0228) * (1.0 + 0.0228 * m).ln();
        let astar = mstar * hue_radians.cos();
        let bstar = mstar * hue_radians.sin();

        Self::new(hue, chroma, j, q, m, s, jstar, astar, bstar)
    }

    /// Create CAM16 from J (lightness), C (chroma), and h (hue) with default viewing conditions.
    pub fn from_jch(j: f64, c: f64, h: f64) -> Self {
        Self::from_jch_in_viewing_conditions(j, c, h, &ViewingConditions::default())
    }

    /// Create CAM16 from J (lightness), C (chroma), and h (hue) with specific viewing conditions.
    pub fn from_jch_in_viewing_conditions(j: f64, c: f64, h: f64, vc: &ViewingConditions) -> Self {
        let q = (4.0 / vc.c) * (j / 100.0).sqrt() * (vc.aw + 4.0) * vc.fl_root;
        let m = c * vc.fl_root;
        let alpha = c / (j / 100.0).sqrt();
        let s = 50.0 * ((alpha * vc.c) / (vc.aw + 4.0)).sqrt();

        let hue_radians = h * std::f64::consts::PI / 180.0;
        let jstar = ((1.0 + 100.0 * 0.007) * j) / (1.0 + 0.007 * j);
        let mstar = (1.0 / 0.0228) * (1.0 + 0.0228 * m).ln();
        let astar = mstar * hue_radians.cos();
        let bstar = mstar * hue_radians.sin();

        Self::new(h, c, j, q, m, s, jstar, astar, bstar)
    }

    /// Create CAM16 from CAM16-UCS coordinates (J*, a*, b*) with default viewing conditions.
    pub fn from_ucs(jstar: f64, astar: f64, bstar: f64) -> Self {
        Self::from_ucs_in_viewing_conditions(jstar, astar, bstar, &ViewingConditions::default())
    }

    /// Create CAM16 from CAM16-UCS coordinates (J*, a*, b*) with specific viewing conditions.
    pub fn from_ucs_in_viewing_conditions(
        jstar: f64,
        astar: f64,
        bstar: f64,
        vc: &ViewingConditions,
    ) -> Self {
        let m = (astar * astar + bstar * bstar).sqrt();
        let big_m = ((m * 0.0228).exp() - 1.0) / 0.0228;
        let c = big_m / vc.fl_root;

        let mut h = bstar.atan2(astar) * (180.0 / std::f64::consts::PI);
        if h < 0.0 {
            h += 360.0;
        }

        let j = jstar / (1.0 - (jstar - 100.0) * 0.007);
        Self::from_jch_in_viewing_conditions(j, c, h, vc)
    }

    /// Convert to ARGB, assuming default viewing conditions.
    pub fn to_int(&self) -> u32 {
        self.viewed(&ViewingConditions::default())
    }

    /// Convert to ARGB with specific viewing conditions.
    pub fn viewed(&self, vc: &ViewingConditions) -> u32 {
        let xyz = self.xyz_in_viewing_conditions(vc);
        argb_from_xyz(xyz[0], xyz[1], xyz[2])
    }

    /// Get XYZ representation with specific viewing conditions.
    pub fn xyz_in_viewing_conditions(&self, vc: &ViewingConditions) -> [f64; 3] {
        let alpha = if self.chroma == 0.0 || self.j == 0.0 {
            0.0
        } else {
            self.chroma / (self.j / 100.0).sqrt()
        };

        let t = (alpha / (1.64 - 0.29_f64.powf(vc.n)).powf(0.73)).powf(1.0 / 0.9);
        let h_rad = self.hue * std::f64::consts::PI / 180.0;

        let e_hue = 0.25 * ((h_rad + 2.0).cos() + 3.8);
        let ac = vc.aw * (self.j / 100.0).powf(1.0 / vc.c / vc.z);
        let p1 = e_hue * (50000.0 / 13.0) * vc.nc * vc.ncb;
        let p2 = ac / vc.nbb;

        let h_sin = h_rad.sin();
        let h_cos = h_rad.cos();

        let gamma = (23.0 * (p2 + 0.305) * t) / (23.0 * p1 + 11.0 * t * h_cos + 108.0 * t * h_sin);
        let a = gamma * h_cos;
        let b = gamma * h_sin;

        let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
        let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
        let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;

        let r_c_base = (27.13 * r_a.abs() / (400.0 - r_a.abs())).max(0.0);
        let r_c = (signum(r_a) as f64) * (100.0 / vc.fl) * r_c_base.powf(1.0 / 0.42);

        let g_c_base = (27.13 * g_a.abs() / (400.0 - g_a.abs())).max(0.0);
        let g_c = (signum(g_a) as f64) * (100.0 / vc.fl) * g_c_base.powf(1.0 / 0.42);

        let b_c_base = (27.13 * b_a.abs() / (400.0 - b_a.abs())).max(0.0);
        let b_c = (signum(b_a) as f64) * (100.0 / vc.fl) * b_c_base.powf(1.0 / 0.42);

        let r_f = r_c / vc.rgb_d[0];
        let g_f = g_c / vc.rgb_d[1];
        let b_f = b_c / vc.rgb_d[2];

        // Inverse M16 matrix
        let x = 1.86206786 * r_f - 1.01125463 * g_f + 0.14918677 * b_f;
        let y = 0.38752654 * r_f + 0.62144744 * g_f - 0.00897398 * b_f;
        let z = -0.01584150 * r_f - 0.03412294 * g_f + 1.04996444 * b_f;

        [x, y, z]
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
    const WHITE: u32 = 0xFFFFFFFF;
    const BLACK: u32 = 0xFF000000;

    #[test]
    fn test_red_hue() {
        let cam = Cam16::from_int(RED);
        assert_relative_eq!(cam.hue, 27.408, epsilon = 0.001);
    }

    #[test]
    fn test_red_chroma() {
        let cam = Cam16::from_int(RED);
        assert_relative_eq!(cam.chroma, 113.357, epsilon = 0.001);
    }

    #[test]
    fn test_red_j() {
        let cam = Cam16::from_int(RED);
        assert_relative_eq!(cam.j, 46.445, epsilon = 0.001);
    }

    #[test]
    fn test_red_m() {
        let cam = Cam16::from_int(RED);
        assert_relative_eq!(cam.m, 89.494, epsilon = 0.001);
    }

    #[test]
    fn test_red_s() {
        let cam = Cam16::from_int(RED);
        assert_relative_eq!(cam.s, 91.889, epsilon = 0.001);
    }

    #[test]
    fn test_red_q() {
        let cam = Cam16::from_int(RED);
        assert_relative_eq!(cam.q, 105.988, epsilon = 0.001);
    }

    #[test]
    fn test_green_hue() {
        let cam = Cam16::from_int(GREEN);
        assert_relative_eq!(cam.hue, 142.139, epsilon = 0.001);
    }

    #[test]
    fn test_green_chroma() {
        let cam = Cam16::from_int(GREEN);
        assert_relative_eq!(cam.chroma, 108.410, epsilon = 0.001);
    }

    #[test]
    fn test_green_j() {
        let cam = Cam16::from_int(GREEN);
        assert_relative_eq!(cam.j, 79.331, epsilon = 0.001);
    }

    #[test]
    fn test_green_m() {
        let cam = Cam16::from_int(GREEN);
        assert_relative_eq!(cam.m, 85.587, epsilon = 0.001);
    }

    #[test]
    fn test_green_s() {
        let cam = Cam16::from_int(GREEN);
        assert_relative_eq!(cam.s, 78.604, epsilon = 0.001);
    }

    #[test]
    fn test_green_q() {
        let cam = Cam16::from_int(GREEN);
        assert_relative_eq!(cam.q, 138.520, epsilon = 0.001);
    }

    #[test]
    fn test_blue_hue() {
        let cam = Cam16::from_int(BLUE);
        assert_relative_eq!(cam.hue, 282.788, epsilon = 0.001);
    }

    #[test]
    fn test_blue_chroma() {
        let cam = Cam16::from_int(BLUE);
        assert_relative_eq!(cam.chroma, 87.230, epsilon = 0.001);
    }

    #[test]
    fn test_blue_j() {
        let cam = Cam16::from_int(BLUE);
        assert_relative_eq!(cam.j, 25.465, epsilon = 0.001);
    }

    #[test]
    fn test_blue_m() {
        let cam = Cam16::from_int(BLUE);
        assert_relative_eq!(cam.m, 68.867, epsilon = 0.001);
    }

    #[test]
    fn test_blue_s() {
        let cam = Cam16::from_int(BLUE);
        assert_relative_eq!(cam.s, 93.674, epsilon = 0.001);
    }

    #[test]
    fn test_blue_q() {
        let cam = Cam16::from_int(BLUE);
        assert_relative_eq!(cam.q, 78.481, epsilon = 0.001);
    }

    #[test]
    fn test_white_hue() {
        let cam = Cam16::from_int(WHITE);
        assert_relative_eq!(cam.hue, 209.492, epsilon = 0.001);
    }

    #[test]
    fn test_white_chroma() {
        let cam = Cam16::from_int(WHITE);
        assert_relative_eq!(cam.chroma, 2.869, epsilon = 0.001);
    }

    #[test]
    fn test_white_j() {
        let cam = Cam16::from_int(WHITE);
        assert_relative_eq!(cam.j, 100.0, epsilon = 0.001);
    }

    #[test]
    fn test_white_m() {
        let cam = Cam16::from_int(WHITE);
        assert_relative_eq!(cam.m, 2.265, epsilon = 0.001);
    }

    #[test]
    fn test_white_s() {
        let cam = Cam16::from_int(WHITE);
        assert_relative_eq!(cam.s, 12.068, epsilon = 0.001);
    }

    #[test]
    fn test_white_q() {
        let cam = Cam16::from_int(WHITE);
        assert_relative_eq!(cam.q, 155.521, epsilon = 0.001);
    }

    #[test]
    fn test_black_hue() {
        let cam = Cam16::from_int(BLACK);
        assert_relative_eq!(cam.hue, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_black_chroma() {
        let cam = Cam16::from_int(BLACK);
        assert_relative_eq!(cam.chroma, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_black_j() {
        let cam = Cam16::from_int(BLACK);
        assert_relative_eq!(cam.j, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_black_m() {
        let cam = Cam16::from_int(BLACK);
        assert_relative_eq!(cam.m, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_black_s() {
        let cam = Cam16::from_int(BLACK);
        assert_relative_eq!(cam.s, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_black_q() {
        let cam = Cam16::from_int(BLACK);
        assert_relative_eq!(cam.q, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_red_round_trip() {
        let cam = Cam16::from_int(RED);
        let argb = cam.to_int();
        assert_eq!(argb, RED);
    }

    #[test]
    fn test_green_round_trip() {
        let cam = Cam16::from_int(GREEN);
        let argb = cam.to_int();
        assert_eq!(argb, GREEN);
    }

    #[test]
    fn test_blue_round_trip() {
        let cam = Cam16::from_int(BLUE);
        let argb = cam.to_int();
        assert_eq!(argb, BLUE);
    }

    #[test]
    fn test_white_round_trip() {
        let cam = Cam16::from_int(WHITE);
        let argb = cam.to_int();
        assert_eq!(argb, WHITE);
    }

    #[test]
    fn test_black_round_trip() {
        let cam = Cam16::from_int(BLACK);
        let argb = cam.to_int();
        assert_eq!(argb, BLACK);
    }

    #[test]
    fn test_from_jch() {
        let cam1 = Cam16::from_int(RED);
        let cam2 = Cam16::from_jch(cam1.j, cam1.chroma, cam1.hue);
        assert_relative_eq!(cam2.j, cam1.j, epsilon = 0.001);
        assert_relative_eq!(cam2.chroma, cam1.chroma, epsilon = 0.001);
        assert_relative_eq!(cam2.hue, cam1.hue, epsilon = 0.001);
    }

    #[test]
    fn test_from_ucs() {
        let cam1 = Cam16::from_int(RED);
        let cam2 = Cam16::from_ucs(cam1.jstar, cam1.astar, cam1.bstar);
        assert_relative_eq!(cam2.jstar, cam1.jstar, epsilon = 0.001);
        assert_relative_eq!(cam2.astar, cam1.astar, epsilon = 0.001);
        assert_relative_eq!(cam2.bstar, cam1.bstar, epsilon = 0.001);
    }

    #[test]
    fn test_distance_same_color() {
        let cam = Cam16::from_int(RED);
        assert_relative_eq!(cam.distance(&cam), 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_distance_different_colors() {
        let red = Cam16::from_int(RED);
        let blue = Cam16::from_int(BLUE);
        // Distance should be positive and non-trivial
        assert!(red.distance(&blue) > 10.0);
    }

    #[test]
    fn test_distance_symmetric() {
        let red = Cam16::from_int(RED);
        let green = Cam16::from_int(GREEN);
        assert_relative_eq!(red.distance(&green), green.distance(&red), epsilon = 0.001);
    }

    #[test]
    fn test_clone() {
        let cam1 = Cam16::from_int(RED);
        let cam2 = cam1;
        assert_relative_eq!(cam1.hue, cam2.hue, epsilon = 0.001);
    }

    #[test]
    fn test_debug_impl() {
        let cam = Cam16::from_int(RED);
        let debug_str = format!("{:?}", cam);
        assert!(debug_str.contains("Cam16"));
    }
}

// <FILE>crates/mcu-hct/src/cam16.rs</FILE> - <DESC>CAM16 color appearance model implementation</DESC>
// <VERS>END OF VERSION: 1.0.0</VERS>
