use gpui::{Corners, Pixels, px};

/// M3 Expressive Spring Tokens & Physics Solver
///
/// In Material Design 3 Expressive (AndroidX `ExpressiveMotionTokens.kt`):
/// - `FastSpatial`: dampingRatio = 0.6, stiffness = 800.0 (used for interactive shape morphing)
/// - `DefaultSpatial`: dampingRatio = 0.8, stiffness = 380.0
/// - `FastEffects`: dampingRatio = 1.0, stiffness = 3800.0
#[derive(Clone, Copy, Debug)]
pub struct ExpressiveSpring {
    pub damping: f32,
    pub stiffness: f32,
}

impl Default for ExpressiveSpring {
    fn default() -> Self {
        Self::fast_spatial()
    }
}

impl ExpressiveSpring {
    /// FastSpatial spring spec (damping = 0.6, stiffness = 800.0)
    /// Produces a responsive shape morph with subtle spring bounciness.
    pub const fn fast_spatial() -> Self {
        Self {
            damping: 0.6,
            stiffness: 800.0,
        }
    }

    /// DefaultSpatial spring spec (damping = 0.8, stiffness = 380.0)
    pub const fn default_spatial() -> Self {
        Self {
            damping: 0.8,
            stiffness: 380.0,
        }
    }

    /// FastEffects spring spec (damping = 1.0, stiffness = 3800.0)
    pub const fn fast_effects() -> Self {
        Self {
            damping: 1.0,
            stiffness: 3800.0,
        }
    }

    /// Evaluates the spring position at elapsed time `t` (in seconds)
    /// for a transition from 0.0 to 1.0.
    pub fn evaluate(&self, t_seconds: f32) -> f32 {
        if t_seconds <= 0.0 {
            return 0.0;
        }

        let omega_n = self.stiffness.sqrt();
        let zeta = self.damping;

        if (zeta - 1.0).abs() < 1e-4 {
            // Critically damped (zeta = 1.0)
            // x(t) = 1 - (1 + omega_n * t) * exp(-omega_n * t)
            1.0 - (1.0 + omega_n * t_seconds) * (-omega_n * t_seconds).exp()
        } else if zeta < 1.0 {
            // Underdamped (zeta < 1.0, e.g. 0.6 for FastSpatial)
            let omega_d = omega_n * (1.0 - zeta * zeta).sqrt();
            let alpha = zeta * omega_n;
            let beta = alpha / omega_d;
            // x(t) = 1 - exp(-alpha * t) * (cos(omega_d * t) + beta * sin(omega_d * t))
            1.0 - (-alpha * t_seconds).exp() * ((omega_d * t_seconds).cos() + beta * (omega_d * t_seconds).sin())
        } else {
            // Overdamped (zeta > 1.0)
            let gamma = (zeta * zeta - 1.0).sqrt();
            let r1 = -omega_n * (zeta - gamma);
            let r2 = -omega_n * (zeta + gamma);
            1.0 - (r2 * (r1 * t_seconds).exp() - r1 * (r2 * t_seconds).exp()) / (r2 - r1)
        }
    }
}

/// Linearly interpolates between two corner radii specs based on spring `progress`
pub fn lerp_corners(start: Corners<Pixels>, target: Corners<Pixels>, progress: f32) -> Corners<Pixels> {
    let lerp_val = |a: Pixels, b: Pixels| -> Pixels {
        let a_f = f32::from(a);
        let b_f = f32::from(b);
        px(a_f + (b_f - a_f) * progress)
    };

    Corners {
        top_left: lerp_val(start.top_left, target.top_left),
        top_right: lerp_val(start.top_right, target.top_right),
        bottom_right: lerp_val(start.bottom_right, target.bottom_right),
        bottom_left: lerp_val(start.bottom_left, target.bottom_left),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expressive_spring_fast_spatial() {
        let spring = ExpressiveSpring::fast_spatial();
        assert_eq!(spring.evaluate(0.0), 0.0);
        
        let p_mid = spring.evaluate(0.05);
        assert!(p_mid > 0.4 && p_mid < 0.7);

        // Underdamped spring at 0.15s should show subtle overshoot (> 1.0)
        let p_over = spring.evaluate(0.15);
        assert!(p_over >= 1.0);

        // Settles near 1.0
        let p_end = spring.evaluate(0.35);
        assert!((p_end - 1.0).abs() < 0.02);
    }

    #[test]
    fn test_lerp_corners() {
        let start = Corners::all(px(24.0));
        let target = Corners::all(px(12.0));
        let mid = lerp_corners(start, target, 0.5);
        assert_eq!(mid.top_left, px(18.0));
        assert_eq!(mid.top_right, px(18.0));
        assert_eq!(mid.bottom_right, px(18.0));
        assert_eq!(mid.bottom_left, px(18.0));
    }
}
