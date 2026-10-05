/// Returns -1 if num < 0, 0 if num == 0, 1 if num > 0.
pub fn signum(num: f64) -> i32 {
    if num < 0.0 {
        -1
    } else if num == 0.0 {
        0
    } else {
        1
    }
}

/// Linear interpolation between start and stop by amount.
/// Returns start when amount=0, stop when amount=1.
pub fn lerp(start: f64, stop: f64, amount: f64) -> f64 {
    (1.0 - amount) * start + amount * stop
}

/// Clamps an integer between min and max (inclusive).
pub fn clamp_int(min: i32, max: i32, input: i32) -> i32 {
    if input < min {
        min
    } else if input > max {
        max
    } else {
        input
    }
}

/// Clamps a float between min and max (inclusive).
pub fn clamp_double(min: f64, max: f64, input: f64) -> f64 {
    if input < min {
        min
    } else if input > max {
        max
    } else {
        input
    }
}

/// Sanitizes a degree measure as an integer to range [0, 360).
pub fn sanitize_degrees_int(degrees: i32) -> i32 {
    let mut deg = degrees % 360;
    if deg < 0 {
        deg += 360;
    }
    deg
}

/// Sanitizes a degree measure as a float to range [0.0, 360.0).
pub fn sanitize_degrees_double(degrees: f64) -> f64 {
    let mut deg = degrees % 360.0;
    if deg < 0.0 {
        deg += 360.0;
    }
    deg
}

/// Returns the direction to rotate from one angle to another.
/// Returns 1.0 if increasing, -1.0 if decreasing is shorter.
/// For 180 degrees apart, returns 1.0.
pub fn rotation_direction(from: f64, to: f64) -> f64 {
    let increasing_difference = sanitize_degrees_double(to - from);
    if increasing_difference <= 180.0 {
        1.0
    } else {
        -1.0
    }
}

/// Angular distance between two degree values.
pub fn difference_degrees(a: f64, b: f64) -> f64 {
    180.0 - ((a - b).abs() - 180.0).abs()
}

/// Multiplies a 1x3 row vector with a 3x3 matrix.
pub fn matrix_multiply(row: [f64; 3], matrix: [[f64; 3]; 3]) -> [f64; 3] {
    [
        row[0] * matrix[0][0] + row[1] * matrix[0][1] + row[2] * matrix[0][2],
        row[0] * matrix[1][0] + row[1] * matrix[1][1] + row[2] * matrix[1][2],
        row[0] * matrix[2][0] + row[1] * matrix[2][1] + row[2] * matrix[2][2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signum_positive() {
        assert_eq!(signum(5.0), 1);
        assert_eq!(signum(0.001), 1);
    }

    #[test]
    fn test_signum_negative() {
        assert_eq!(signum(-5.0), -1);
        assert_eq!(signum(-0.001), -1);
    }

    #[test]
    fn test_signum_zero() {
        assert_eq!(signum(0.0), 0);
    }

    #[test]
    fn test_lerp_start() {
        assert!((lerp(0.0, 100.0, 0.0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_lerp_end() {
        assert!((lerp(0.0, 100.0, 1.0) - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_lerp_middle() {
        assert!((lerp(0.0, 100.0, 0.5) - 50.0).abs() < 1e-10);
    }

    #[test]
    fn test_clamp_int_within() {
        assert_eq!(clamp_int(0, 100, 50), 50);
    }

    #[test]
    fn test_clamp_int_below() {
        assert_eq!(clamp_int(0, 100, -10), 0);
    }

    #[test]
    fn test_clamp_int_above() {
        assert_eq!(clamp_int(0, 100, 150), 100);
    }

    #[test]
    fn test_clamp_double_within() {
        assert!((clamp_double(0.0, 1.0, 0.5) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_clamp_double_below() {
        assert!((clamp_double(0.0, 1.0, -0.5) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_clamp_double_above() {
        assert!((clamp_double(0.0, 1.0, 1.5) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_sanitize_degrees_int_positive() {
        assert_eq!(sanitize_degrees_int(450), 90);
    }

    #[test]
    fn test_sanitize_degrees_int_negative() {
        assert_eq!(sanitize_degrees_int(-90), 270);
    }

    #[test]
    fn test_sanitize_degrees_int_zero() {
        assert_eq!(sanitize_degrees_int(0), 0);
    }

    #[test]
    fn test_sanitize_degrees_double_positive() {
        assert!((sanitize_degrees_double(450.0) - 90.0).abs() < 1e-10);
    }

    #[test]
    fn test_sanitize_degrees_double_negative() {
        assert!((sanitize_degrees_double(-90.0) - 270.0).abs() < 1e-10);
    }

    #[test]
    fn test_rotation_direction_increasing() {
        assert!((rotation_direction(0.0, 90.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_rotation_direction_decreasing() {
        assert!((rotation_direction(0.0, 270.0) - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_rotation_direction_180() {
        assert!((rotation_direction(0.0, 180.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_difference_degrees_same() {
        assert!((difference_degrees(0.0, 0.0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_difference_degrees_opposite() {
        assert!((difference_degrees(0.0, 180.0) - 180.0).abs() < 1e-10);
    }

    #[test]
    fn test_difference_degrees_wrap() {
        assert!((difference_degrees(350.0, 10.0) - 20.0).abs() < 1e-10);
    }

    #[test]
    fn test_matrix_multiply_identity() {
        let row = [1.0, 2.0, 3.0];
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let result = matrix_multiply(row, identity);
        assert!((result[0] - 1.0).abs() < 1e-10);
        assert!((result[1] - 2.0).abs() < 1e-10);
        assert!((result[2] - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_matrix_multiply_scale() {
        let row = [1.0, 1.0, 1.0];
        let scale = [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]];
        let result = matrix_multiply(row, scale);
        assert!((result[0] - 2.0).abs() < 1e-10);
        assert!((result[1] - 3.0).abs() < 1e-10);
        assert!((result[2] - 4.0).abs() < 1e-10);
    }
}
