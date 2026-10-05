use std::fmt;

use super::cam16::Cam16;
use super::hct_solver::HctSolver;
use super::viewing_conditions::ViewingConditions;
use crate::material::color::utils::color::{lstar_from_argb, lstar_from_y};

/// HCT, hue, chroma, and tone.
///
/// A color system that provides a perceptually accurate color measurement system
/// that can also accurately render what colors will appear as in different
/// lighting environments.
///
/// Using L* creates a link between the color system, contrast, and thus
/// accessibility. Contrast ratio depends on relative luminance, or Y in the XYZ
/// color space. L*, or perceptual luminance can be calculated from Y.
///
/// Unlike Y, L* is linear to human perception, allowing trivial creation of
/// accurate color tones.
///
/// Unlike contrast ratio, measuring contrast in L* is linear, and simple to
/// calculate. A difference of 40 in HCT tone guarantees a contrast ratio >= 3.0,
/// and a difference of 50 guarantees a contrast ratio >= 4.5.
#[derive(Debug, Clone, Copy)]
pub struct Hct {
    hue: f64,
    chroma: f64,
    tone: f64,
    argb: u32,
}

impl Hct {
    /// Create an HCT color from hue, chroma, and tone.
    ///
    /// # Arguments
    ///
    /// * `hue` - 0 <= hue < 360; invalid values are corrected.
    /// * `chroma` - 0 <= chroma < ?; Informally, colorfulness. The color
    ///   returned may be lower than the requested chroma. Chroma has a different
    ///   maximum for any given hue and tone.
    /// * `tone` - 0 <= tone <= 100; invalid values are corrected.
    ///
    /// Returns HCT representation of a color in default viewing conditions.
    pub fn from(hue: f64, chroma: f64, tone: f64) -> Self {
        let argb = HctSolver::solve_to_int(hue, chroma, tone);
        Self::from_int(argb)
    }

    /// Create an HCT color from an ARGB integer.
    ///
    /// # Arguments
    ///
    /// * `argb` - ARGB representation of a color.
    ///
    /// Returns HCT representation of a color in default viewing conditions.
    pub fn from_int(argb: u32) -> Self {
        let cam = Cam16::from_int(argb);
        Self {
            hue: cam.hue,
            chroma: cam.chroma,
            tone: lstar_from_argb(argb),
            argb,
        }
    }

    /// Convert this HCT color to an ARGB integer.
    pub fn to_int(&self) -> u32 {
        self.argb
    }

    /// Get the hue component (0-360 degrees).
    pub fn hue(&self) -> f64 {
        self.hue
    }

    /// Get the chroma component.
    pub fn chroma(&self) -> f64 {
        self.chroma
    }

    /// Get the tone component (0-100 lightness).
    pub fn tone(&self) -> f64 {
        self.tone
    }

    /// Set the hue component.
    ///
    /// Chroma may decrease because chroma has a different maximum for any given
    /// hue and tone.
    pub fn set_hue(&mut self, new_hue: f64) {
        self.set_internal_state(HctSolver::solve_to_int(new_hue, self.chroma, self.tone));
    }

    /// Set the chroma component.
    ///
    /// Chroma may decrease because chroma has a different maximum for any given
    /// hue and tone.
    pub fn set_chroma(&mut self, new_chroma: f64) {
        self.set_internal_state(HctSolver::solve_to_int(self.hue, new_chroma, self.tone));
    }

    /// Set the tone component.
    ///
    /// Chroma may decrease because chroma has a different maximum for any given
    /// hue and tone.
    pub fn set_tone(&mut self, new_tone: f64) {
        self.set_internal_state(HctSolver::solve_to_int(self.hue, self.chroma, new_tone));
    }

    /// Create a new HCT with a different hue.
    pub fn with_hue(self, new_hue: f64) -> Self {
        Self::from(new_hue, self.chroma, self.tone)
    }

    /// Create a new HCT with a different chroma.
    pub fn with_chroma(self, new_chroma: f64) -> Self {
        Self::from(self.hue, new_chroma, self.tone)
    }

    /// Create a new HCT with a different tone.
    pub fn with_tone(self, new_tone: f64) -> Self {
        Self::from(self.hue, self.chroma, new_tone)
    }

    /// Update internal state from a new ARGB value.
    fn set_internal_state(&mut self, argb: u32) {
        let cam = Cam16::from_int(argb);
        self.hue = cam.hue;
        self.chroma = cam.chroma;
        self.tone = lstar_from_argb(argb);
        self.argb = argb;
    }

    /// Check if a hue falls in the blue range.
    pub fn is_blue(hue: f64) -> bool {
        (250.0..270.0).contains(&hue)
    }

    /// Check if a hue falls in the yellow range.
    pub fn is_yellow(hue: f64) -> bool {
        (105.0..125.0).contains(&hue)
    }

    /// Check if a hue falls in the cyan range.
    pub fn is_cyan(hue: f64) -> bool {
        (170.0..207.0).contains(&hue)
    }

    /// Translates a color into different viewing conditions.
    ///
    /// Colors change appearance. They look different with lights on versus off,
    /// the same color, as in hex code, on white looks different when on black.
    /// This is called color relativity, most famously explicated by Josef Albers
    /// in Interaction of Color.
    ///
    /// In color science, color appearance models can account for this and
    /// calculate the appearance of a color in different settings. HCT is based on
    /// CAM16, a color appearance model, and uses it to make these calculations.
    pub fn in_viewing_conditions(&self, vc: &ViewingConditions) -> Self {
        // 1. Use CAM16 to find XYZ coordinates of color in specified VC.
        let cam = Cam16::from_int(self.to_int());
        let viewed_in_vc = cam.xyz_in_viewing_conditions(vc);

        // 2. Create CAM16 of those XYZ coordinates in default VC.
        let recast_in_vc = Cam16::from_xyz_in_viewing_conditions(
            viewed_in_vc[0],
            viewed_in_vc[1],
            viewed_in_vc[2],
            &ViewingConditions::default(),
        );

        // 3. Create HCT from:
        // - CAM16 using default VC with XYZ coordinates in specified VC.
        // - L* converted from Y in XYZ coordinates in specified VC.
        Self::from(
            recast_in_vc.hue,
            recast_in_vc.chroma,
            lstar_from_y(viewed_in_vc[1]),
        )
    }
}

impl fmt::Display for Hct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HCT({:.0}, {:.0}, {:.0})",
            self.hue, self.chroma, self.tone
        )
    }
}

impl PartialEq for Hct {
    fn eq(&self, other: &Self) -> bool {
        self.argb == other.argb
    }
}

impl Eq for Hct {}

impl std::hash::Hash for Hct {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.argb.hash(state);
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
    fn test_from_int_red() {
        let hct = Hct::from_int(RED);
        assert_relative_eq!(hct.hue(), 27.408, epsilon = 0.001);
        assert_relative_eq!(hct.chroma(), 113.357, epsilon = 0.001);
    }

    #[test]
    fn test_from_int_green() {
        let hct = Hct::from_int(GREEN);
        assert_relative_eq!(hct.hue(), 142.139, epsilon = 0.001);
        assert_relative_eq!(hct.chroma(), 108.410, epsilon = 0.001);
    }

    #[test]
    fn test_from_int_blue() {
        let hct = Hct::from_int(BLUE);
        assert_relative_eq!(hct.hue(), 282.788, epsilon = 0.001);
        assert_relative_eq!(hct.chroma(), 87.230, epsilon = 0.001);
    }

    #[test]
    fn test_to_int_preserves_color() {
        let hct = Hct::from_int(RED);
        assert_eq!(hct.to_int(), RED);
    }

    #[test]
    fn test_from_int_preserves_alpha_but_generated_hct_is_opaque() {
        let transparent = Hct::from_int(0x00123456);
        let opaque = Hct::from_int(0xff123456);

        assert_eq!(transparent.to_int(), 0x00123456);
        assert_eq!(transparent.hue(), opaque.hue());
        assert_eq!(transparent.chroma(), opaque.chroma());
        assert_eq!(transparent.tone(), opaque.tone());
        assert_eq!(Hct::from(27.0, 80.0, 50.0).to_int() >> 24, 0xff);
    }

    #[test]
    fn test_in_viewing_conditions_matches_reference_recast() {
        let vc = ViewingConditions::make(
            crate::material::color::utils::color::white_point_d65(),
            11.725677948856951,
            30.0,
            1.0,
            false,
        );
        let viewed = Hct::from_int(0xff6750a4).in_viewing_conditions(&vc);

        assert_eq!(viewed.to_int(), 0xff4d3a7d);
        assert_relative_eq!(viewed.hue(), 299.42810508917745, epsilon = 1e-12);
        assert_relative_eq!(viewed.chroma(), 41.232459757928865, epsilon = 1e-12);
        assert_relative_eq!(viewed.tone(), 29.624810435520317, epsilon = 1e-12);
    }

    #[test]
    fn test_round_trip_red() {
        let hct = Hct::from_int(RED);
        let argb = Hct::from(hct.hue(), hct.chroma(), hct.tone()).to_int();
        assert_eq!(argb, RED);
    }

    #[test]
    fn test_round_trip_green() {
        let hct = Hct::from_int(GREEN);
        let argb = Hct::from(hct.hue(), hct.chroma(), hct.tone()).to_int();
        assert_eq!(argb, GREEN);
    }

    #[test]
    fn test_round_trip_blue() {
        let hct = Hct::from_int(BLUE);
        let argb = Hct::from(hct.hue(), hct.chroma(), hct.tone()).to_int();
        assert_eq!(argb, BLUE);
    }

    #[test]
    fn test_white_tone() {
        let hct = Hct::from_int(WHITE);
        assert_relative_eq!(hct.tone(), 100.0, epsilon = 0.001);
    }

    #[test]
    fn test_black_tone() {
        let hct = Hct::from_int(BLACK);
        assert_relative_eq!(hct.tone(), 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_set_hue() {
        let mut hct = Hct::from_int(RED);
        let original_chroma = hct.chroma();
        let original_tone = hct.tone();
        hct.set_hue(180.0);
        // Tone should be preserved
        assert_relative_eq!(hct.tone(), original_tone, epsilon = 1.0);
        // Hue should be close to requested
        assert!((hct.hue() - 180.0).abs() < 10.0 || hct.chroma() < original_chroma);
    }

    #[test]
    fn test_set_tone() {
        let mut hct = Hct::from_int(RED);
        let original_hue = hct.hue();
        hct.set_tone(50.0);
        // Tone should be close to requested
        assert_relative_eq!(hct.tone(), 50.0, epsilon = 1.0);
        // Hue should be preserved
        assert_relative_eq!(hct.hue(), original_hue, epsilon = 1.0);
    }

    #[test]
    fn test_set_chroma() {
        let mut hct = Hct::from_int(RED);
        let original_hue = hct.hue();
        let original_tone = hct.tone();
        hct.set_chroma(50.0);
        // Chroma should be close to or below requested
        assert!(hct.chroma() <= 51.0);
        // Hue and tone should be preserved
        assert_relative_eq!(hct.hue(), original_hue, epsilon = 1.0);
        assert_relative_eq!(hct.tone(), original_tone, epsilon = 1.0);
    }

    #[test]
    fn test_with_hue() {
        let hct = Hct::from_int(RED);
        let new_hct = hct.with_hue(180.0);
        // Original unchanged
        assert_relative_eq!(hct.hue(), 27.408, epsilon = 0.001);
        // New has different hue
        assert!(new_hct.hue() != hct.hue() || new_hct.chroma() < hct.chroma());
    }

    #[test]
    fn test_with_tone() {
        let hct = Hct::from_int(RED);
        let new_hct = hct.with_tone(50.0);
        assert_relative_eq!(new_hct.tone(), 50.0, epsilon = 1.0);
    }

    #[test]
    fn test_with_chroma() {
        let hct = Hct::from_int(RED);
        let new_hct = hct.with_chroma(50.0);
        assert!(new_hct.chroma() <= 51.0);
    }

    #[test]
    fn test_is_blue() {
        assert!(!Hct::is_blue(249.0));
        assert!(Hct::is_blue(250.0));
        assert!(Hct::is_blue(260.0));
        assert!(Hct::is_blue(269.0));
        assert!(!Hct::is_blue(270.0));
    }

    #[test]
    fn test_is_yellow() {
        assert!(!Hct::is_yellow(104.0));
        assert!(Hct::is_yellow(105.0));
        assert!(Hct::is_yellow(115.0));
        assert!(Hct::is_yellow(124.0));
        assert!(!Hct::is_yellow(125.0));
    }

    #[test]
    fn test_is_cyan() {
        assert!(!Hct::is_cyan(169.0));
        assert!(Hct::is_cyan(170.0));
        assert!(Hct::is_cyan(190.0));
        assert!(Hct::is_cyan(206.0));
        assert!(!Hct::is_cyan(207.0));
    }

    #[test]
    fn test_display() {
        let hct = Hct::from(180.0, 50.0, 50.0);
        let display = format!("{}", hct);
        assert!(display.contains("HCT"));
    }

    #[test]
    fn test_equality() {
        let hct1 = Hct::from_int(RED);
        let hct2 = Hct::from_int(RED);
        let hct3 = Hct::from_int(BLUE);
        assert_eq!(hct1, hct2);
        assert_ne!(hct1, hct3);
    }

    #[test]
    fn test_clone() {
        let hct1 = Hct::from_int(RED);
        let hct2 = hct1;
        assert_eq!(hct1, hct2);
    }

    #[test]
    fn test_debug() {
        let hct = Hct::from_int(RED);
        let debug = format!("{:?}", hct);
        assert!(debug.contains("Hct"));
    }

    #[test]
    fn test_from_creates_valid_hct() {
        for tone in (0..=100).step_by(10) {
            for hue in (0..360).step_by(30) {
                let hct = Hct::from(hue as f64, 50.0, tone as f64);
                // Tone should be preserved
                assert_relative_eq!(hct.tone(), tone as f64, epsilon = 1.0);
                // Hue should be close (may shift if chroma is reduced)
                if hct.chroma() > 5.0 {
                    let hue_diff = (hct.hue() - hue as f64).abs();
                    let hue_diff = hue_diff.min(360.0 - hue_diff);
                    assert!(
                        hue_diff < 15.0,
                        "Hue diff {} for requested hue {}",
                        hue_diff,
                        hue
                    );
                }
            }
        }
    }
}
