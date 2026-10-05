use crate::material::color::hct::Hct;

/// Check and/or fix universally disliked colors.
///
/// Color science studies of color preference indicate universal distaste for
/// dark yellow-greens, and also show this is correlated to distaste for
/// biological waste and rotting food.
///
/// See Palmer and Schloss, 2010 or Schloss and Palmer's Chapter 21 in Handbook
/// of Color Psychology (2015).
pub struct DislikeAnalyzer;

impl DislikeAnalyzer {
    /// Returns true if a color is disliked.
    ///
    /// # Arguments
    ///
    /// * `hct` - A color to be judged.
    ///
    /// # Returns
    ///
    /// Whether the color is disliked.
    ///
    /// Disliked is defined as a dark yellow-green that is not neutral.
    /// Specifically:
    /// - Hue in range [90, 111] (rounded)
    /// - Chroma > 16 (rounded)
    /// - Tone < 65 (rounded)
    pub fn is_disliked(hct: &Hct) -> bool {
        let hue_passes = hct.hue().round() >= 90.0 && hct.hue().round() <= 111.0;
        let chroma_passes = hct.chroma().round() > 16.0;
        let tone_passes = hct.tone().round() < 65.0;

        hue_passes && chroma_passes && tone_passes
    }

    /// If a color is disliked, lighten it to make it likable.
    ///
    /// # Arguments
    ///
    /// * `hct` - A color to be judged.
    ///
    /// # Returns
    ///
    /// A new color if the original color is disliked, or the original
    /// color if it is acceptable.
    ///
    /// The fix works by lightening the color to tone 70, which moves it
    /// out of the "dark" range that triggers the dislike response.
    pub fn fix_if_disliked(hct: &Hct) -> Hct {
        if Self::is_disliked(hct) {
            Hct::from(hct.hue(), hct.chroma(), 70.0)
        } else {
            *hct
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_disliked_dark_yellow_green() {
        let hct = Hct::from(100.0, 50.0, 40.0);
        assert!(DislikeAnalyzer::is_disliked(&hct));
    }

    #[test]
    fn test_not_disliked_light_color() {
        let hct = Hct::from(100.0, 50.0, 70.0);
        assert!(!DislikeAnalyzer::is_disliked(&hct));
    }

    #[test]
    fn test_fix_if_disliked_changes_tone() {
        let hct = Hct::from(100.0, 50.0, 40.0);
        let fixed = DislikeAnalyzer::fix_if_disliked(&hct);
        assert!((fixed.tone() - 70.0).abs() < 1.0);
    }

    #[test]
    fn test_fix_if_disliked_preserves_liked_color() {
        let hct = Hct::from(200.0, 50.0, 50.0);
        let fixed = DislikeAnalyzer::fix_if_disliked(&hct);
        assert_eq!(fixed.to_int(), hct.to_int());
    }
}
