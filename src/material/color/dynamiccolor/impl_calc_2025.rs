
//! 2025 Material Design specification color calculation delegate.
//!
//! This module contains the tone and HCT calculation logic for the updated
//! Material Design 3 specification (2025), which includes DeltaConstraint handling,
//! different awkward zone (57-65), and special _fixed_dim handling.

use crate::material::color::contrast::Contrast;
use crate::material::color::hct::Hct;
use crate::material::color::palettes::TonalPalette;
use crate::material::color::utils::math::clamp_double;

use super::color_calculation::ColorCalculationDelegate;
use super::{DeltaConstraint, DynamicColor, DynamicScheme, TonePolarity};

/// Color calculation delegate for the 2025 Material Design spec.
///
/// Extends the 2021 algorithm with:
/// - Dim variant support
/// - Platform-specific adjustments (phone vs watch)
/// - Additional tone calculation refinements
/// - DeltaConstraint handling (exact/nearer/farther)
/// - Different awkward zone (57-65 vs 2021's 50-60)
#[derive(Debug, Clone, Copy, Default)]
pub struct ColorCalculationDelegate2025;

impl ColorCalculationDelegate for ColorCalculationDelegate2025 {
    fn get_hct(&self, scheme: &DynamicScheme, color: &DynamicColor) -> Hct {
        // Match the canonical 2025 path: the palette's ARGB tone conversion is
        // used when chroma is unchanged, including the yellow T99 adjustment.
        let palette = (color.palette)(scheme);
        let tone = self.get_tone(scheme, color);
        let multiplier = color
            .chroma_multiplier
            .as_ref()
            .map(|f| f(scheme))
            .unwrap_or(1.0);
        if multiplier == 1.0 {
            return palette.get_hct(tone);
        }
        let chroma = palette.chroma() * multiplier;
        if tone == 99.0 && Hct::is_yellow(palette.hue()) {
            return TonalPalette::from_hue_and_chroma(palette.hue(), chroma).get_hct(tone);
        }
        Hct::from(palette.hue(), chroma, tone)
    }

    fn get_tone(&self, scheme: &DynamicScheme, color: &DynamicColor) -> f64 {
        // Apply the 2025 tone-delta and constraint rules to select the role tone.
        let tone_delta_pair = color.tone_delta_pair.as_ref().and_then(|f| f(scheme));

        // CASE 0: ToneDeltaPair with constraint handling (2025 spec uses constraint field)
        if let Some(pair) = tone_delta_pair {
            let role_a = &pair.role_a;
            let role_b = &pair.role_b;
            let polarity = pair.polarity;
            let constraint = pair.constraint;

            // Calculate absolute delta based on polarity
            // Darker polarities result in negative delta
            let absolute_delta = if matches!(polarity, TonePolarity::Darker)
                || (matches!(polarity, TonePolarity::RelativeLighter) && scheme.is_dark)
                || (matches!(polarity, TonePolarity::RelativeDarker) && !scheme.is_dark)
            {
                -pair.delta
            } else {
                pair.delta
            };

            // Determine which role we are (A or B) and get the reference role
            let am_role_a = color.name == role_a.name;
            let self_role = if am_role_a { role_a } else { role_b };
            let ref_role = if am_role_a { role_b } else { role_a };

            // The reference role's tone includes its own contrast/pair
            // resolution, matching the canonical 2025 algorithm.
            let mut self_tone = (self_role.tone)(scheme);
            let ref_tone = ref_role.get_tone(scheme);

            // Relative delta flips sign if we're role B
            let relative_delta = absolute_delta * if am_role_a { 1.0 } else { -1.0 };

            // Apply constraint (2025 spec: exact/nearer/farther)
            match constraint {
                DeltaConstraint::Exact => {
                    // Exact: set self_tone to exactly ref_tone + relative_delta
                    self_tone = clamp_double(0.0, 100.0, ref_tone + relative_delta);
                }
                DeltaConstraint::Nearer => {
                    // Nearer: clamp self_tone to be within the delta range of ref_tone
                    if relative_delta > 0.0 {
                        self_tone = clamp_double(
                            0.0,
                            100.0,
                            clamp_double(ref_tone, ref_tone + relative_delta, self_tone),
                        );
                    } else {
                        self_tone = clamp_double(
                            0.0,
                            100.0,
                            clamp_double(ref_tone + relative_delta, ref_tone, self_tone),
                        );
                    }
                }
                DeltaConstraint::Farther => {
                    // Farther: ensure self_tone is at least delta away from ref_tone
                    if relative_delta > 0.0 {
                        self_tone = clamp_double(ref_tone + relative_delta, 100.0, self_tone);
                    } else {
                        self_tone = clamp_double(0.0, ref_tone + relative_delta, self_tone);
                    }
                }
            }

            // Contrast adjustment if background exists
            if let (Some(bg_fn), Some(curve_fn)) = (&color.background, &color.contrast_curve) {
                if let (Some(bg), Some(curve)) = (bg_fn(scheme), curve_fn(scheme)) {
                    let bg_tone = bg.get_tone(scheme);
                    let self_contrast = curve.get(scheme.contrast_level);
                    // Keep the tone only when contrast is sufficient and level is nonnegative.
                    if !(Contrast::ratio_of_tones(bg_tone, self_tone) >= self_contrast
                        && scheme.contrast_level >= 0.0)
                    {
                        self_tone = DynamicColor::foreground_tone(bg_tone, self_contrast);
                    }
                }
            }

            // 2025 awkward zone: 57-65 (different from 2021's 50-60)
            // Don't adjust _fixed_dim colors
            if color.is_background && !color.name.ends_with("_fixed_dim") {
                if self_tone >= 57.0 {
                    self_tone = clamp_double(65.0, 100.0, self_tone);
                } else {
                    self_tone = clamp_double(0.0, 49.0, self_tone);
                }
            }

            return self_tone;
        }

        // CASE 1: No ToneDeltaPair - just solve for itself
        let mut answer = (color.tone)(scheme);

        let bg_opt = color.background.as_ref().and_then(|f| f(scheme));
        let curve_opt = color.contrast_curve.as_ref().and_then(|f| f(scheme));

        // No adjustment for colors with no background
        if bg_opt.is_none() || curve_opt.is_none() {
            return answer;
        }

        let bg = bg_opt.unwrap();
        let curve = curve_opt.unwrap();
        let bg_tone = bg.get_tone(scheme);
        let desired_ratio = curve.get(scheme.contrast_level);

        // Keep the tone only when contrast is sufficient and level is nonnegative.
        if !(Contrast::ratio_of_tones(bg_tone, answer) >= desired_ratio
            && scheme.contrast_level >= 0.0)
        {
            answer = DynamicColor::foreground_tone(bg_tone, desired_ratio);
        }

        // 2025 awkward zone handling (57-65)
        // Don't adjust _fixed_dim colors
        if color.is_background && !color.name.ends_with("_fixed_dim") {
            if answer >= 57.0 {
                answer = clamp_double(65.0, 100.0, answer);
            } else {
                answer = clamp_double(0.0, 49.0, answer);
            }
        }

        // CASE 2: Dual backgrounds
        let second_bg_opt = color.second_background.as_ref().and_then(|f| f(scheme));
        if second_bg_opt.is_none() {
            return answer;
        }

        let bg1_tone = bg_tone;
        let bg2_tone = second_bg_opt.unwrap().get_tone(scheme);
        let upper = bg1_tone.max(bg2_tone);
        let lower = bg1_tone.min(bg2_tone);

        // If current answer satisfies contrast with both backgrounds, we're done
        if Contrast::ratio_of_tones(upper, answer) >= desired_ratio
            && Contrast::ratio_of_tones(lower, answer) >= desired_ratio
        {
            return answer;
        }

        // Find tones that satisfy contrast with both backgrounds
        // lighter_option: darkest light tone that works, or -1 if impossible
        let light_option = Contrast::lighter(upper, desired_ratio);
        // darker_option: lightest dark tone that works, or -1 if impossible
        let dark_option = Contrast::darker(lower, desired_ratio);

        // Determine preference based on background tones
        let prefers_light = DynamicColor::tone_prefers_light_foreground(bg1_tone)
            || DynamicColor::tone_prefers_light_foreground(bg2_tone);

        if prefers_light {
            if light_option < 0.0 {
                100.0
            } else {
                light_option
            }
        } else if dark_option >= 0.0 {
            dark_option
        } else {
            0.0
        }
    }
}
