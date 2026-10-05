// SPDX-License-Identifier: Apache-2.0
// Copyright 2021 Google LLC
// Copyright 2026 JAC and Contributors
//
// Adapted from Google's Material Color Utilities (commit
// 5b3618b16fdc3825e21d5679bafd144662088ea1) via the mcu-material-color Rust port.
// Changes: embedded in the Amane algorithm module and imports made crate-local.
// <FILE>crates/mcu-utils/src/color.rs</FILE> - <DESC>Color space conversion utilities (ARGB, XYZ, Lab, L*)</DESC>
// <VERS>VERSION: 1.0.0</VERS>
// <WCTX>Port color utilities from TypeScript Material Color Utilities</WCTX>
// <CLOG>Implement ARGB component extraction, XYZ/Lab/L* conversions, linearization/delinearization</CLOG>

use super::math::{clamp_int, matrix_multiply};

/// sRGB to XYZ conversion matrix.
pub const SRGB_TO_XYZ: [[f64; 3]; 3] = [
    [0.41233895, 0.35762064, 0.18051042],
    [0.2126, 0.7152, 0.0722],
    [0.01932141, 0.11916382, 0.95034478],
];

/// XYZ to sRGB conversion matrix.
pub const XYZ_TO_SRGB: [[f64; 3]; 3] = [
    [
        3.2413774792388685,
        -1.5376652402851851,
        -0.49885366846268053,
    ],
    [-0.9691452513005321, 1.8758853451067872, 0.04156585616912061],
    [
        0.05562093689691305,
        -0.20395524564742123,
        1.0571799111220335,
    ],
];

/// D65 white point (standard daylight).
pub const WHITE_POINT_D65: [f64; 3] = [95.047, 100.0, 108.883];

/// Converts RGB components to ARGB format with full opacity.
///
/// # Arguments
/// * `red` - Red component (0-255)
/// * `green` - Green component (0-255)
/// * `blue` - Blue component (0-255)
///
/// # Returns
/// ARGB color as u32 (0xAARRGGBB format)
#[inline]
pub fn argb_from_rgb(red: u8, green: u8, blue: u8) -> u32 {
    0xFF000000 | ((red as u32) << 16) | ((green as u32) << 8) | (blue as u32)
}

/// Converts linear RGB components to ARGB format.
///
/// # Arguments
/// * `linrgb` - Linear RGB components (0-100 scale each)
///
/// # Returns
/// ARGB color as u32 (0xAARRGGBB format)
pub fn argb_from_linrgb(linrgb: [f64; 3]) -> u32 {
    let r = delinearized(linrgb[0]);
    let g = delinearized(linrgb[1]);
    let b = delinearized(linrgb[2]);
    argb_from_rgb(r, g, b)
}

/// Extracts the alpha component from an ARGB color.
#[inline]
pub fn alpha_from_argb(argb: u32) -> u8 {
    ((argb >> 24) & 0xFF) as u8
}

/// Extracts the red component from an ARGB color.
#[inline]
pub fn red_from_argb(argb: u32) -> u8 {
    ((argb >> 16) & 0xFF) as u8
}

/// Extracts the green component from an ARGB color.
#[inline]
pub fn green_from_argb(argb: u32) -> u8 {
    ((argb >> 8) & 0xFF) as u8
}

/// Extracts the blue component from an ARGB color.
#[inline]
pub fn blue_from_argb(argb: u32) -> u8 {
    (argb & 0xFF) as u8
}

/// Returns whether an ARGB color is fully opaque.
#[inline]
pub fn is_opaque(argb: u32) -> bool {
    alpha_from_argb(argb) == 255
}

/// Converts XYZ color to ARGB format.
///
/// # Arguments
/// * `x`, `y`, `z` - XYZ color components
///
/// # Returns
/// ARGB color as u32
pub fn argb_from_xyz(x: f64, y: f64, z: f64) -> u32 {
    let matrix = XYZ_TO_SRGB;
    let linear_r = matrix[0][0] * x + matrix[0][1] * y + matrix[0][2] * z;
    let linear_g = matrix[1][0] * x + matrix[1][1] * y + matrix[1][2] * z;
    let linear_b = matrix[2][0] * x + matrix[2][1] * y + matrix[2][2] * z;
    let r = delinearized(linear_r);
    let g = delinearized(linear_g);
    let b = delinearized(linear_b);
    argb_from_rgb(r, g, b)
}

/// Converts ARGB color to XYZ color space.
///
/// # Arguments
/// * `argb` - ARGB color as u32
///
/// # Returns
/// XYZ color components as [x, y, z]
pub fn xyz_from_argb(argb: u32) -> [f64; 3] {
    let r = linearized(red_from_argb(argb));
    let g = linearized(green_from_argb(argb));
    let b = linearized(blue_from_argb(argb));
    matrix_multiply([r, g, b], SRGB_TO_XYZ)
}

/// Converts Lab color to ARGB format.
///
/// # Arguments
/// * `l` - L* (lightness, 0-100)
/// * `a` - a* (green-red axis)
/// * `b` - b* (blue-yellow axis)
///
/// # Returns
/// ARGB color as u32
pub fn argb_from_lab(l: f64, a: f64, b: f64) -> u32 {
    let white_point = WHITE_POINT_D65;
    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;
    let x_normalized = lab_invf(fx);
    let y_normalized = lab_invf(fy);
    let z_normalized = lab_invf(fz);
    let x = x_normalized * white_point[0];
    let y = y_normalized * white_point[1];
    let z = z_normalized * white_point[2];
    argb_from_xyz(x, y, z)
}

/// Converts ARGB color to Lab color space.
///
/// # Arguments
/// * `argb` - ARGB color as u32
///
/// # Returns
/// Lab color components as [l, a, b]
pub fn lab_from_argb(argb: u32) -> [f64; 3] {
    let linear_r = linearized(red_from_argb(argb));
    let linear_g = linearized(green_from_argb(argb));
    let linear_b = linearized(blue_from_argb(argb));
    let matrix = SRGB_TO_XYZ;
    let x = matrix[0][0] * linear_r + matrix[0][1] * linear_g + matrix[0][2] * linear_b;
    let y = matrix[1][0] * linear_r + matrix[1][1] * linear_g + matrix[1][2] * linear_b;
    let z = matrix[2][0] * linear_r + matrix[2][1] * linear_g + matrix[2][2] * linear_b;
    let white_point = WHITE_POINT_D65;
    let x_normalized = x / white_point[0];
    let y_normalized = y / white_point[1];
    let z_normalized = z / white_point[2];
    let fx = lab_f(x_normalized);
    let fy = lab_f(y_normalized);
    let fz = lab_f(z_normalized);
    let l = 116.0 * fy - 16.0;
    let a = 500.0 * (fx - fy);
    let b = 200.0 * (fy - fz);
    [l, a, b]
}

/// Converts an L* value to a grayscale ARGB color.
///
/// # Arguments
/// * `lstar` - L* lightness value (0-100)
///
/// # Returns
/// ARGB grayscale color matching the L* lightness
pub fn argb_from_lstar(lstar: f64) -> u32 {
    let y = y_from_lstar(lstar);
    let component = delinearized(y);
    argb_from_rgb(component, component, component)
}

/// Computes the L* value of an ARGB color.
///
/// # Arguments
/// * `argb` - ARGB color as u32
///
/// # Returns
/// L* lightness value (0-100)
pub fn lstar_from_argb(argb: u32) -> f64 {
    let y = xyz_from_argb(argb)[1];
    116.0 * lab_f(y / 100.0) - 16.0
}

/// Converts L* to Y (luminance).
///
/// L* in L*a*b* and Y in XYZ both measure luminance.
/// L* is perceptual (linear scale), Y is relative (logarithmic scale).
///
/// # Arguments
/// * `lstar` - L* lightness value
///
/// # Returns
/// Y luminance value (0-100)
pub fn y_from_lstar(lstar: f64) -> f64 {
    100.0 * lab_invf((lstar + 16.0) / 116.0)
}

/// Converts Y (luminance) to L*.
///
/// # Arguments
/// * `y` - Y luminance value (0-100)
///
/// # Returns
/// L* lightness value (0-100)
pub fn lstar_from_y(y: f64) -> f64 {
    lab_f(y / 100.0) * 116.0 - 16.0
}

/// Linearizes an sRGB component to linear RGB (0-100 scale).
///
/// # Arguments
/// * `rgb_component` - sRGB component (0-255)
///
/// # Returns
/// Linear RGB value (0-100)
pub fn linearized(rgb_component: u8) -> f64 {
    let normalized = rgb_component as f64 / 255.0;
    if normalized <= 0.040449936 {
        normalized / 12.92 * 100.0
    } else {
        ((normalized + 0.055) / 1.055).powf(2.4) * 100.0
    }
}

/// Delinearizes a linear RGB component to sRGB (0-255 scale).
///
/// # Arguments
/// * `rgb_component` - Linear RGB value (0-100)
///
/// # Returns
/// sRGB component (0-255)
pub fn delinearized(rgb_component: f64) -> u8 {
    let normalized = rgb_component / 100.0;
    let delinearized = if normalized <= 0.0031308 {
        normalized * 12.92
    } else {
        1.055 * normalized.powf(1.0 / 2.4) - 0.055
    };
    clamp_int(0, 255, (delinearized * 255.0).round() as i32) as u8
}

/// Returns the D65 white point.
#[inline]
pub fn white_point_d65() -> [f64; 3] {
    WHITE_POINT_D65
}

/// Lab f function for converting to Lab space.
fn lab_f(t: f64) -> f64 {
    const E: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    if t > E {
        t.powf(1.0 / 3.0)
    } else {
        (KAPPA * t + 16.0) / 116.0
    }
}

/// Lab inverse f function for converting from Lab space.
fn lab_invf(ft: f64) -> f64 {
    const E: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    let ft3 = ft * ft * ft;
    if ft3 > E {
        ft3
    } else {
        (116.0 * ft - 16.0) / KAPPA
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ARGB Component Tests ====================

    #[test]
    fn test_argb_from_rgb_black() {
        assert_eq!(argb_from_rgb(0, 0, 0), 0xFF000000);
    }

    #[test]
    fn test_argb_from_rgb_white() {
        assert_eq!(argb_from_rgb(255, 255, 255), 0xFFFFFFFF);
    }

    #[test]
    fn test_argb_from_rgb_red() {
        assert_eq!(argb_from_rgb(255, 0, 0), 0xFFFF0000);
    }

    #[test]
    fn test_argb_from_rgb_green() {
        assert_eq!(argb_from_rgb(0, 255, 0), 0xFF00FF00);
    }

    #[test]
    fn test_argb_from_rgb_blue() {
        assert_eq!(argb_from_rgb(0, 0, 255), 0xFF0000FF);
    }

    #[test]
    fn test_alpha_from_argb() {
        assert_eq!(alpha_from_argb(0xFF123456), 255);
        assert_eq!(alpha_from_argb(0x80123456), 128);
        assert_eq!(alpha_from_argb(0x00123456), 0);
    }

    #[test]
    fn test_red_from_argb() {
        assert_eq!(red_from_argb(0xFF123456), 0x12);
        assert_eq!(red_from_argb(0xFFFF0000), 255);
        assert_eq!(red_from_argb(0xFF000000), 0);
    }

    #[test]
    fn test_green_from_argb() {
        assert_eq!(green_from_argb(0xFF123456), 0x34);
        assert_eq!(green_from_argb(0xFF00FF00), 255);
        assert_eq!(green_from_argb(0xFF000000), 0);
    }

    #[test]
    fn test_blue_from_argb() {
        assert_eq!(blue_from_argb(0xFF123456), 0x56);
        assert_eq!(blue_from_argb(0xFF0000FF), 255);
        assert_eq!(blue_from_argb(0xFF000000), 0);
    }

    #[test]
    fn test_is_opaque() {
        assert!(is_opaque(0xFFFFFFFF));
        assert!(is_opaque(0xFF000000));
        assert!(!is_opaque(0xFEFFFFFF));
        assert!(!is_opaque(0x00000000));
    }

    // ==================== Linearization Tests ====================

    #[test]
    fn test_linearized_black() {
        assert!((linearized(0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_linearized_white() {
        assert!((linearized(255) - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_linearized_midgray() {
        // sRGB 128 should give approximately 21.4 in linear (0-100 scale)
        let linear = linearized(128);
        assert!(linear > 20.0 && linear < 23.0);
    }

    #[test]
    fn test_delinearized_black() {
        assert_eq!(delinearized(0.0), 0);
    }

    #[test]
    fn test_delinearized_white() {
        assert_eq!(delinearized(100.0), 255);
    }

    #[test]
    fn test_linearized_delinearized_roundtrip() {
        for i in 0..=255 {
            let linear = linearized(i);
            let back = delinearized(linear);
            assert_eq!(back, i, "Roundtrip failed for {}", i);
        }
    }

    // ==================== XYZ Conversion Tests ====================

    #[test]
    fn test_xyz_from_argb_white() {
        let xyz = xyz_from_argb(0xFFFFFFFF);
        // White should be close to D65 white point (95.047, 100, 108.883)
        assert!((xyz[0] - 95.047).abs() < 0.5);
        assert!((xyz[1] - 100.0).abs() < 0.5);
        assert!((xyz[2] - 108.883).abs() < 0.5);
    }

    #[test]
    fn test_xyz_from_argb_black() {
        let xyz = xyz_from_argb(0xFF000000);
        assert!((xyz[0] - 0.0).abs() < 1e-10);
        assert!((xyz[1] - 0.0).abs() < 1e-10);
        assert!((xyz[2] - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_argb_from_xyz_white() {
        let argb = argb_from_xyz(95.047, 100.0, 108.883);
        assert_eq!(argb, 0xFFFFFFFF);
    }

    #[test]
    fn test_argb_from_xyz_black() {
        let argb = argb_from_xyz(0.0, 0.0, 0.0);
        assert_eq!(argb, 0xFF000000);
    }

    #[test]
    fn test_xyz_roundtrip() {
        let original = 0xFF8080FF; // Light blue
        let xyz = xyz_from_argb(original);
        let back = argb_from_xyz(xyz[0], xyz[1], xyz[2]);
        assert_eq!(back, original);
    }

    // ==================== Lab Conversion Tests ====================

    #[test]
    fn test_lab_from_argb_white() {
        let lab = lab_from_argb(0xFFFFFFFF);
        assert!((lab[0] - 100.0).abs() < 0.1); // L* = 100
        assert!((lab[1] - 0.0).abs() < 0.1); // a* = 0
        assert!((lab[2] - 0.0).abs() < 0.1); // b* = 0
    }

    #[test]
    fn test_lab_from_argb_black() {
        let lab = lab_from_argb(0xFF000000);
        assert!((lab[0] - 0.0).abs() < 0.1); // L* = 0
        assert!((lab[1] - 0.0).abs() < 0.1); // a* = 0
        assert!((lab[2] - 0.0).abs() < 0.1); // b* = 0
    }

    #[test]
    fn test_argb_from_lab_white() {
        let argb = argb_from_lab(100.0, 0.0, 0.0);
        assert_eq!(argb, 0xFFFFFFFF);
    }

    #[test]
    fn test_argb_from_lab_black() {
        let argb = argb_from_lab(0.0, 0.0, 0.0);
        assert_eq!(argb, 0xFF000000);
    }

    #[test]
    fn test_lab_roundtrip() {
        let original = 0xFF4080C0; // Arbitrary color
        let lab = lab_from_argb(original);
        let back = argb_from_lab(lab[0], lab[1], lab[2]);
        // Allow small rounding differences
        assert_eq!(red_from_argb(back), red_from_argb(original));
        assert_eq!(green_from_argb(back), green_from_argb(original));
        assert_eq!(blue_from_argb(back), blue_from_argb(original));
    }

    // ==================== L* Conversion Tests ====================

    #[test]
    fn test_lstar_from_argb_white() {
        let lstar = lstar_from_argb(0xFFFFFFFF);
        assert!((lstar - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_lstar_from_argb_black() {
        let lstar = lstar_from_argb(0xFF000000);
        assert!((lstar - 0.0).abs() < 0.1);
    }

    #[test]
    fn test_argb_from_lstar_white() {
        let argb = argb_from_lstar(100.0);
        assert_eq!(argb, 0xFFFFFFFF);
    }

    #[test]
    fn test_argb_from_lstar_black() {
        let argb = argb_from_lstar(0.0);
        assert_eq!(argb, 0xFF000000);
    }

    #[test]
    fn test_argb_from_lstar_midgray() {
        let argb = argb_from_lstar(50.0);
        let r = red_from_argb(argb);
        let g = green_from_argb(argb);
        let b = blue_from_argb(argb);
        // Should be a gray value
        assert_eq!(r, g);
        assert_eq!(g, b);
        // L* 50 should be approximately sRGB 119
        assert!((r as i32 - 119).abs() < 2);
    }

    // ==================== Y and L* Tests ====================

    #[test]
    fn test_y_from_lstar_0() {
        let y = y_from_lstar(0.0);
        assert!((y - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_y_from_lstar_100() {
        let y = y_from_lstar(100.0);
        assert!((y - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_lstar_from_y_0() {
        let lstar = lstar_from_y(0.0);
        assert!((lstar - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_lstar_from_y_100() {
        let lstar = lstar_from_y(100.0);
        assert!((lstar - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_y_lstar_roundtrip() {
        for i in 0..=100 {
            let lstar = i as f64;
            let y = y_from_lstar(lstar);
            let back = lstar_from_y(y);
            assert!(
                (back - lstar).abs() < 0.001,
                "Roundtrip failed for L* {}",
                lstar
            );
        }
    }

    // ==================== White Point Test ====================

    #[test]
    fn test_white_point_d65() {
        let wp = white_point_d65();
        assert!((wp[0] - 95.047).abs() < 1e-10);
        assert!((wp[1] - 100.0).abs() < 1e-10);
        assert!((wp[2] - 108.883).abs() < 1e-10);
    }

    // ==================== Linear RGB Tests ====================

    #[test]
    fn test_argb_from_linrgb_white() {
        let argb = argb_from_linrgb([100.0, 100.0, 100.0]);
        assert_eq!(argb, 0xFFFFFFFF);
    }

    #[test]
    fn test_argb_from_linrgb_black() {
        let argb = argb_from_linrgb([0.0, 0.0, 0.0]);
        assert_eq!(argb, 0xFF000000);
    }

    #[test]
    fn test_argb_from_linrgb_red() {
        let argb = argb_from_linrgb([100.0, 0.0, 0.0]);
        assert_eq!(argb, 0xFFFF0000);
    }
}

// <FILE>crates/mcu-utils/src/color.rs</FILE> - <DESC>Color space conversion utilities (ARGB, XYZ, Lab, L*)</DESC>
// <VERS>END OF VERSION: 1.0.0</VERS>
