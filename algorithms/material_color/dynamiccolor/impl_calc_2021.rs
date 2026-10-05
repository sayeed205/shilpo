// <FILE>crates/mcu-dynamiccolor/src/impl_calc_2021.rs</FILE> - <DESC>2021 spec color calculation delegate implementation</DESC>
// <VERS>VERSION: 1.0.0</VERS>
// <WCTX>OFPF refactor: Extract 2021 calculation delegate from color_calculation.rs</WCTX>
// <CLOG>Initial extraction of ColorCalculationDelegate2021 to comply with OFPF LOC limits</CLOG>

//! 2021 Material Design specification color calculation delegate.
//!
//! This module contains the tone and HCT calculation logic for the original
//! Material Design 3 specification (2021).

use crate::material_color::contrast::Contrast;
use crate::material_color::hct::Hct;
use crate::material_color::utils::math::clamp_double;

use super::color_calculation::ColorCalculationDelegate;
use super::{DynamicColor, DynamicScheme, TonePolarity};

/// Color calculation delegate for the 2021 Material Design spec.
///
/// Implements the original Material Design 3 color calculation algorithm with:
/// - Contrast curve based adjustments
/// - Tone delta pair constraints
/// - Dual background handling
#[derive(Debug, Clone, Copy, Default)]
pub struct ColorCalculationDelegate2021;

impl ColorCalculationDelegate for ColorCalculationDelegate2021 {
    fn get_hct(&self, scheme: &DynamicScheme, color: &DynamicColor) -> Hct {
        // The canonical HCT path retains fractional role tones in TonalPalette;
        // only ARGB conversion quantizes to displayable channels.
        let tone = self.get_tone(scheme, color);
        let palette = (color.palette)(scheme);
        palette.get_hct(tone)
    }

    fn get_tone(&self, scheme: &DynamicScheme, color: &DynamicColor) -> f64 {
        // Full 2021 spec algorithm ported from TypeScript dynamic_color.ts:409-582
        let decreasing_contrast = scheme.contrast_level < 0.0;
        let tone_delta_pair = color.tone_delta_pair.as_ref().and_then(|f| f(scheme));

        // CASE 1: ToneDeltaPair exists - pair of colors with delta constraint
        if let Some(pair) = tone_delta_pair {
            let role_a = &pair.role_a;
            let role_b = &pair.role_b;
            let delta = pair.delta;
            let polarity = pair.polarity;
            let stay_together = pair.stay_together;

            // Determine which role is "nearer" based on polarity and dark mode
            let a_is_nearer = matches!(polarity, TonePolarity::Nearer)
                || (matches!(polarity, TonePolarity::Lighter) && !scheme.is_dark)
                || (matches!(polarity, TonePolarity::Darker) && scheme.is_dark);

            let (nearer, farther) = if a_is_nearer {
                (role_a, role_b)
            } else {
                (role_b, role_a)
            };
            let am_nearer = color.name == nearer.name;
            let expansion_dir = if scheme.is_dark { 1.0 } else { -1.0 };

            let mut n_tone = (nearer.tone)(scheme);
            let mut f_tone = (farther.tone)(scheme);

            // Round 1: Apply contrast minimums if background and contrast curves exist
            if let (Some(bg_fn), Some(n_curve_fn), Some(f_curve_fn)) = (
                &color.background,
                &nearer.contrast_curve,
                &farther.contrast_curve,
            ) {
                if let (Some(bg), Some(n_curve), Some(f_curve)) =
                    (bg_fn(scheme), n_curve_fn(scheme), f_curve_fn(scheme))
                {
                    let bg_tone = bg.get_tone(scheme);
                    let n_contrast = n_curve.get(scheme.contrast_level);
                    let f_contrast = f_curve.get(scheme.contrast_level);

                    // Adjust nearer if contrast is insufficient
                    if Contrast::ratio_of_tones(bg_tone, n_tone) < n_contrast {
                        n_tone = DynamicColor::foreground_tone(bg_tone, n_contrast);
                    }
                    // Adjust farther if contrast is insufficient
                    if Contrast::ratio_of_tones(bg_tone, f_tone) < f_contrast {
                        f_tone = DynamicColor::foreground_tone(bg_tone, f_contrast);
                    }
                    // If decreasing contrast, adjust both to bare minimum
                    if decreasing_contrast {
                        n_tone = DynamicColor::foreground_tone(bg_tone, n_contrast);
                        f_tone = DynamicColor::foreground_tone(bg_tone, f_contrast);
                    }
                }
            }

            // Round 2: Expand farther if delta not satisfied
            if (f_tone - n_tone) * expansion_dir < delta {
                f_tone = clamp_double(0.0, 100.0, n_tone + delta * expansion_dir);
                // Round 3: Contract nearer if still not satisfied
                if (f_tone - n_tone) * expansion_dir < delta {
                    n_tone = clamp_double(0.0, 100.0, f_tone - delta * expansion_dir);
                }
            }

            // Awkward zone handling (T50-59)
            if n_tone >= 50.0 && n_tone < 60.0 {
                // If nearer is in awkward zone, move it away together with farther
                if expansion_dir > 0.0 {
                    n_tone = 60.0;
                    f_tone = f_tone.max(n_tone + delta * expansion_dir);
                } else {
                    n_tone = 49.0;
                    f_tone = f_tone.min(n_tone + delta * expansion_dir);
                }
            } else if f_tone >= 50.0 && f_tone < 60.0 {
                if stay_together {
                    // Fix both to avoid colors on opposite sides of awkward zone
                    if expansion_dir > 0.0 {
                        n_tone = 60.0;
                        f_tone = f_tone.max(n_tone + delta * expansion_dir);
                    } else {
                        n_tone = 49.0;
                        f_tone = f_tone.min(n_tone + delta * expansion_dir);
                    }
                } else {
                    // Not required to stay together; fix just farther
                    f_tone = if expansion_dir > 0.0 { 60.0 } else { 49.0 };
                }
            }

            return if am_nearer { n_tone } else { f_tone };
        }

        // CASE 2: No ToneDeltaPair - single color contrast adjustment
        let mut answer = (color.tone)(scheme);

        let bg_opt = color.background.as_ref().and_then(|f| f(scheme));
        let curve_opt = color.contrast_curve.as_ref().and_then(|f| f(scheme));

        if bg_opt.is_none() || curve_opt.is_none() {
            return answer; // No adjustment for colors with no background
        }

        let bg = bg_opt.unwrap();
        let curve = curve_opt.unwrap();
        let bg_tone = bg.get_tone(scheme);
        let desired_ratio = curve.get(scheme.contrast_level);

        if Contrast::ratio_of_tones(bg_tone, answer) < desired_ratio {
            answer = DynamicColor::foreground_tone(bg_tone, desired_ratio);
        }
        if decreasing_contrast {
            answer = DynamicColor::foreground_tone(bg_tone, desired_ratio);
        }

        // Awkward zone handling for background colors
        if color.is_background && answer >= 50.0 && answer < 60.0 {
            if Contrast::ratio_of_tones(49.0, bg_tone) >= desired_ratio {
                answer = 49.0;
            } else {
                answer = 60.0;
            }
        }

        // CASE 3: Dual backgrounds
        let second_bg_opt = color.second_background.as_ref().and_then(|f| f(scheme));
        if second_bg_opt.is_none() {
            return answer;
        }

        let bg1_tone = bg_tone;
        let bg2_tone = second_bg_opt.unwrap().get_tone(scheme);
        let upper = bg1_tone.max(bg2_tone);
        let lower = bg1_tone.min(bg2_tone);

        // If answer satisfies both backgrounds, keep it
        if Contrast::ratio_of_tones(upper, answer) >= desired_ratio
            && Contrast::ratio_of_tones(lower, answer) >= desired_ratio
        {
            return answer;
        }

        // Find light/dark options that satisfy the ratio
        let light_option = Contrast::lighter(upper, desired_ratio);
        let dark_option = Contrast::darker(lower, desired_ratio);

        let prefers_light = DynamicColor::tone_prefers_light_foreground(bg1_tone)
            || DynamicColor::tone_prefers_light_foreground(bg2_tone);

        if prefers_light {
            if light_option < 0.0 {
                100.0
            } else {
                light_option
            }
        } else if dark_option >= 0.0 && light_option >= 0.0 {
            dark_option
        } else if dark_option >= 0.0 {
            dark_option
        } else if light_option >= 0.0 {
            light_option
        } else {
            0.0
        }
    }
}

// <FILE>crates/mcu-dynamiccolor/src/impl_calc_2021.rs</FILE> - <DESC>2021 spec color calculation delegate implementation</DESC>
// <VERS>END OF VERSION: 1.0.0</VERS>
