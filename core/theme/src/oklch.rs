//! Pure OKLCH ARGB color interpolation.

/// Interpolate two 0xAARRGGBB colors in OKLCH space.
///
/// - `t` is clamped to `[0.0, 1.0]`. `NaN` and negative infinity return `from`
///   byte-exact; positive infinity returns `to` byte-exact.
/// - `t >= 1.0` returns `to` byte-exact.
/// - Lightness, Chroma, and Alpha are interpolated linearly.
/// - Hue is interpolated along the shortest circular arc across 0°/360°.
/// - Achromatic colors (Chroma < 1e-4) avoid arbitrary hue rotation.
pub fn interpolate_argb_oklch(from: u32, to: u32, t: f32) -> u32 {
    if t.is_nan() || t <= 0.0 {
        return from;
    }
    if t >= 1.0 {
        return to;
    }

    let (a1, r1, g1, b1) = unpack_argb(from);
    let (a2, r2, g2, b2) = unpack_argb(to);

    let (l1, c1, h1) = srgb_to_oklch(r1, g1, b1);
    let (l2, c2, h2) = srgb_to_oklch(r2, g2, b2);

    let a = a1 + t * (a2 - a1);
    let l = l1 + t * (l2 - l1);
    let c = c1 + t * (c2 - c1);

    // Achromatic threshold (chroma below 1e-4)
    const EPSILON: f32 = 1e-4;
    let is_achromatic1 = c1 < EPSILON;
    let is_achromatic2 = c2 < EPSILON;

    let h = if is_achromatic1 && is_achromatic2 {
        0.0
    } else if is_achromatic1 {
        h2
    } else if is_achromatic2 {
        h1
    } else {
        interpolate_hue(h1, h2, t)
    };

    let (r, g, b) = oklch_to_srgb(l, c, h);
    pack_argb(a, r, g, b)
}

#[inline]
fn unpack_argb(argb: u32) -> (f32, f32, f32, f32) {
    let a = ((argb >> 24) & 0xff) as f32 / 255.0;
    let r = ((argb >> 16) & 0xff) as f32 / 255.0;
    let g = ((argb >> 8) & 0xff) as f32 / 255.0;
    let b = (argb & 0xff) as f32 / 255.0;
    (a, r, g, b)
}

#[inline]
fn pack_argb(a: f32, r: f32, g: f32, b: f32) -> u32 {
    let a_u8 = (a.clamp(0.0, 1.0) * 255.0).round() as u8 as u32;
    let r_u8 = (r.clamp(0.0, 1.0) * 255.0).round() as u8 as u32;
    let g_u8 = (g.clamp(0.0, 1.0) * 255.0).round() as u8 as u32;
    let b_u8 = (b.clamp(0.0, 1.0) * 255.0).round() as u8 as u32;
    (a_u8 << 24) | (r_u8 << 16) | (g_u8 << 8) | b_u8
}

fn srgb_to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.max(0.0).powf(1.0 / 2.4) - 0.055
    }
}

#[inline]
#[allow(clippy::excessive_precision)]
fn srgb_to_oklch(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let r_lin = srgb_to_linear(r);
    let g_lin = srgb_to_linear(g);
    let b_lin = srgb_to_linear(b);

    // Oklab M1 matrix: linear sRGB -> LMS
    let l_lms = 0.4122214708 * r_lin + 0.5363325363 * g_lin + 0.0514459929 * b_lin;
    let m_lms = 0.2119034982 * r_lin + 0.6806995451 * g_lin + 0.1073969566 * b_lin;
    let s_lms = 0.0883024619 * r_lin + 0.2817188376 * g_lin + 0.6299787005 * b_lin;

    let l_prime = l_lms.max(0.0).cbrt();
    let m_prime = m_lms.max(0.0).cbrt();
    let s_prime = s_lms.max(0.0).cbrt();

    // Oklab L, a, b
    let l_lab = 0.2104542553 * l_prime + 0.7936177850 * m_prime - 0.0040720468 * s_prime;
    let a_lab = 1.9779984951 * l_prime - 2.4285922050 * m_prime + 0.4505937099 * s_prime;
    let b_lab = 0.0259040371 * l_prime + 0.7827717662 * m_prime - 0.8086757660 * s_prime;

    let chroma = (a_lab * a_lab + b_lab * b_lab).sqrt();
    let hue_rad = b_lab.atan2(a_lab);
    let mut hue_deg = hue_rad.to_degrees();
    if hue_deg < 0.0 {
        hue_deg += 360.0;
    }

    (l_lab, chroma, hue_deg)
}

#[inline]
#[allow(clippy::excessive_precision)]
fn oklch_to_srgb(l: f32, c: f32, h_deg: f32) -> (f32, f32, f32) {
    let h_rad = h_deg.to_radians();
    let a_lab = c * h_rad.cos();
    let b_lab = c * h_rad.sin();

    let l_prime = l + 0.3963377774 * a_lab + 0.2158037573 * b_lab;
    let m_prime = l - 0.1055613458 * a_lab - 0.0638541728 * b_lab;
    let s_prime = l - 0.0894841775 * a_lab - 1.2914855480 * b_lab;

    let l_lms = l_prime * l_prime * l_prime;
    let m_lms = m_prime * m_prime * m_prime;
    let s_lms = s_prime * s_prime * s_prime;

    let r_lin = 4.0767416621 * l_lms - 3.3077115913 * m_lms + 0.2309699292 * s_lms;
    let g_lin = -1.2684380046 * l_lms + 2.6097574011 * m_lms - 0.3413193965 * s_lms;
    let b_lin = -0.0041960863 * l_lms - 0.7034186147 * m_lms + 1.7076147010 * s_lms;

    let r = linear_to_srgb(r_lin);
    let g = linear_to_srgb(g_lin);
    let b = linear_to_srgb(b_lin);

    (r, g, b)
}

fn interpolate_hue(h1: f32, h2: f32, t: f32) -> f32 {
    let mut diff = h2 - h1;
    while diff > 180.0 {
        diff -= 360.0;
    }
    while diff <= -180.0 {
        diff += 360.0;
    }
    let mut h = h1 + t * diff;
    while h >= 360.0 {
        h -= 360.0;
    }
    while h < 0.0 {
        h += 360.0;
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_identity() {
        let from = 0xff6750a4;
        let to = 0xff386a20;

        assert_eq!(interpolate_argb_oklch(from, to, 0.0), from);
        assert_eq!(interpolate_argb_oklch(from, to, 1.0), to);
    }

    #[test]
    fn test_clamping_below_zero_and_above_one() {
        let from = 0xff123456;
        let to = 0xff654321;

        assert_eq!(interpolate_argb_oklch(from, to, -0.5), from);
        assert_eq!(interpolate_argb_oklch(from, to, 1.5), to);
    }

    #[test]
    fn test_non_finite_t() {
        let from = 0xff112233;
        let to = 0xff445566;

        assert_eq!(interpolate_argb_oklch(from, to, f32::NAN), from);
        assert_eq!(interpolate_argb_oklch(from, to, f32::NEG_INFINITY), from);
        assert_eq!(interpolate_argb_oklch(from, to, f32::INFINITY), to);
    }

    #[test]
    fn test_shortest_hue_path_across_zero_degree_boundary() {
        // Pure Magenta (hue ~322 deg) to Red-Orange (hue ~28 deg)
        let magenta_argb = 0xffff00ff;
        let orange_argb = 0xffff3000;

        let (_, r_m, g_m, b_m) = unpack_argb(magenta_argb);
        let (_, r_o, g_o, b_o) = unpack_argb(orange_argb);
        let (_, _, h1) = srgb_to_oklch(r_m, g_m, b_m);
        let (_, _, h2) = srgb_to_oklch(r_o, g_o, b_o);
        assert!(h1 > 300.0, "h1 is {h1}");
        assert!(h2 < 40.0, "h2 is {h2}");

        let mid = interpolate_argb_oklch(magenta_argb, orange_argb, 0.5);
        let (_, r, g, b) = unpack_argb(mid);
        let (_, c, h) = srgb_to_oklch(r, g, b);

        // The shortest path moves 322 -> 360/0 -> 28, midpoint hue should be near 355/0 deg, not 175 deg (cyan)
        assert!(c > 0.05);
        assert!(
            !(30.0..=330.0).contains(&h),
            "midpoint hue {h} should be near 0 deg, not 180 deg (cyan)"
        );
    }

    #[test]
    fn test_achromatic_interpolation() {
        let white = 0xffffffff;
        let black = 0xff000000;
        let red = 0xffff0000;

        // Achromatic ↔ Achromatic
        let gray = interpolate_argb_oklch(black, white, 0.5);
        let (a, r, g, b) = unpack_argb(gray);
        assert_eq!(a, 1.0);
        assert!((r - g).abs() < 0.05);
        assert!((g - b).abs() < 0.05);

        // Achromatic ↔ Chromatic
        let mid_red = interpolate_argb_oklch(white, red, 0.5);
        assert_ne!(mid_red, white);
        assert_ne!(mid_red, red);
    }

    #[test]
    fn test_alpha_interpolation() {
        let transparent_red = 0x00ff0000;
        let opaque_red = 0xffff0000;

        let mid = interpolate_argb_oklch(transparent_red, opaque_red, 0.5);
        let (a, _, _, _) = unpack_argb(mid);
        assert!((a - 0.5).abs() < 0.02);
    }
}
