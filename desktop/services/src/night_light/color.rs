/// Converts a color temperature in Kelvin (1000K to 10000K) to linear RGB multipliers (0.0 to 1.0).
/// Uses Planckian locus approximation (Tanner Helland's algorithm) normalized to 6500K daylight neutral.
pub fn kelvin_to_rgb(kelvin: u32) -> (f64, f64, f64) {
    let kelvin = kelvin.clamp(1000, 10000);
    if kelvin == 6500 {
        return (1.0, 1.0, 1.0);
    }

    let temp = kelvin as f64 / 100.0;

    // Red
    let red = if temp <= 66.0 {
        255.0
    } else {
        329.698727446 * (temp - 60.0).powf(-0.1332047592)
    };

    // Green
    let green = if temp <= 66.0 {
        99.4708025861 * temp.ln() - 161.1195681661
    } else {
        288.1221695283 * (temp - 60.0).powf(-0.0755148492)
    };

    // Blue
    let blue = if temp >= 66.0 {
        255.0
    } else if temp <= 19.0 {
        0.0
    } else {
        138.5177312231 * (temp - 10.0).ln() - 305.0447927307
    };

    // Reference values at 6500K (temp = 65.0)
    let ref_temp = 65.0f64;
    let ref_red = 255.0;
    let ref_green = 99.4708025861 * ref_temp.ln() - 161.1195681661;
    let ref_blue = 138.5177312231 * (ref_temp - 10.0).ln() - 305.0447927307;

    let r = (red.clamp(0.0, 255.0) / ref_red).clamp(0.0, 1.0);
    let g = (green.clamp(0.0, 255.0) / ref_green).clamp(0.0, 1.0);
    let b = (blue.clamp(0.0, 255.0) / ref_blue).clamp(0.0, 1.0);

    (r, g, b)
}

/// Generates native-endian u16 gamma ramp byte buffer for zwlr_gamma_control_v1.
/// Output format: [Red array (gamma_size * 2), Green array (gamma_size * 2), Blue array (gamma_size * 2)].
pub fn generate_gamma_ramp(
    gamma_size: usize,
    r_factor: f64,
    g_factor: f64,
    b_factor: f64,
) -> Vec<u8> {
    if gamma_size == 0 {
        return Vec::new();
    }

    let total_bytes = gamma_size * 2 * 3;
    let mut buf = vec![0u8; total_bytes];

    let max_idx = if gamma_size > 1 {
        (gamma_size - 1) as f64
    } else {
        1.0
    };

    for i in 0..gamma_size {
        let x = i as f64 / max_idx;

        let r_val = (x * r_factor * 65535.0).round().clamp(0.0, 65535.0) as u16;
        let g_val = (x * g_factor * 65535.0).round().clamp(0.0, 65535.0) as u16;
        let b_val = (x * b_factor * 65535.0).round().clamp(0.0, 65535.0) as u16;

        let red_offset = i * 2;
        let green_offset = (gamma_size + i) * 2;
        let blue_offset = (gamma_size * 2 + i) * 2;

        buf[red_offset..red_offset + 2].copy_from_slice(&r_val.to_ne_bytes());
        buf[green_offset..green_offset + 2].copy_from_slice(&g_val.to_ne_bytes());
        buf[blue_offset..blue_offset + 2].copy_from_slice(&b_val.to_ne_bytes());
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kelvin_to_rgb_neutral() {
        let (r, g, b) = kelvin_to_rgb(6500);
        assert_eq!((r, g, b), (1.0, 1.0, 1.0));
    }

    #[test]
    fn test_kelvin_to_rgb_warm() {
        let (r, g, b) = kelvin_to_rgb(3500);
        assert_eq!(r, 1.0);
        assert!(g < 1.0);
        assert!(b < g);
        assert!(b > 0.0);
    }

    #[test]
    fn test_kelvin_to_rgb_bounds() {
        let (r1, g1, b1) = kelvin_to_rgb(1000);
        assert!(r1 > 0.0 && g1 > 0.0);
        assert_eq!(b1, 0.0);

        let (r2, g2, b2) = kelvin_to_rgb(10000);
        assert!(r2 <= 1.0);
        assert!(g2 > 0.0 && g2 <= 1.0);
        assert_eq!(b2, 1.0);
    }

    #[test]
    fn test_generate_gamma_ramp_size() {
        let size = 256;
        let ramp = generate_gamma_ramp(size, 1.0, 0.75, 0.5);
        assert_eq!(ramp.len(), size * 6);

        // Check start (0) and end (size-1) for Red component
        let r_start = u16::from_ne_bytes([ramp[0], ramp[1]]);
        let r_end = u16::from_ne_bytes([ramp[(size - 1) * 2], ramp[(size - 1) * 2 + 1]]);
        assert_eq!(r_start, 0);
        assert_eq!(r_end, 65535);

        // Check end for Blue component
        let b_end_offset = (size * 2 + size - 1) * 2;
        let b_end = u16::from_ne_bytes([ramp[b_end_offset], ramp[b_end_offset + 1]]);
        assert_eq!(b_end, (0.5f64 * 65535.0f64).round() as u16);
    }
}
