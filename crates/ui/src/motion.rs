use gpui::{Corners, Pixels, px};

/// Spring specification for M3 motion animations.
///
/// A physics-based spring defined by `damping` (damping ratio ζ) and `stiffness`.
/// The spring solver supports underdamped (ζ < 1, bouncy), critically damped
/// (ζ = 1, no overshoot), and overdamped (ζ > 1) regimes.
///
/// # Reference
/// AndroidX `ExpressiveMotionTokens.kt` and `StandardMotionTokens.kt`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringSpec {
    /// Damping ratio (ζ). Values < 1.0 produce bounce/overshoot.
    /// 1.0 is critically damped (no overshoot). > 1.0 is overdamped.
    pub damping: f32,
    /// Spring stiffness. Higher values produce faster, snappier motion.
    pub stiffness: f32,
}

impl Default for SpringSpec {
    fn default() -> Self {
        Self::EXPRESSIVE_DEFAULT_SPATIAL
    }
}

impl SpringSpec {
    // ── Expressive Motion Tokens ─────────────────────────────────────────
    // From AndroidX `ExpressiveMotionTokens.kt` v0_14_0.
    // Spatial specs use underdamped springs (ζ < 1) for lively bounce.
    // Effects specs use critically damped springs (ζ = 1) for smooth fades.

    /// Expressive DefaultSpatial (ζ=0.8, k=380) — standard layout/shape changes.
    pub const EXPRESSIVE_DEFAULT_SPATIAL: Self = Self {
        damping: 0.8,
        stiffness: 380.0,
    };
    /// Expressive FastSpatial (ζ=0.6, k=800) — snappy micro-interactions, press morphs.
    pub const EXPRESSIVE_FAST_SPATIAL: Self = Self {
        damping: 0.6,
        stiffness: 800.0,
    };
    /// Expressive SlowSpatial (ζ=0.8, k=200) — page transitions, large layout shifts.
    pub const EXPRESSIVE_SLOW_SPATIAL: Self = Self {
        damping: 0.8,
        stiffness: 200.0,
    };
    /// Expressive DefaultEffects (ζ=1.0, k=1600) — color/opacity state changes.
    pub const EXPRESSIVE_DEFAULT_EFFECTS: Self = Self {
        damping: 1.0,
        stiffness: 1600.0,
    };
    /// Expressive FastEffects (ζ=1.0, k=3800) — hover color, ripple fade.
    pub const EXPRESSIVE_FAST_EFFECTS: Self = Self {
        damping: 1.0,
        stiffness: 3800.0,
    };
    /// Expressive SlowEffects (ζ=1.0, k=800) — background transitions.
    pub const EXPRESSIVE_SLOW_EFFECTS: Self = Self {
        damping: 1.0,
        stiffness: 800.0,
    };

    // ── Standard Motion Tokens ───────────────────────────────────────────
    // From AndroidX `StandardMotionTokens.kt`.
    // Higher damping (ζ=0.9) for functional, subdued motion without bounce.

    /// Standard DefaultSpatial (ζ=0.9, k=700) — functional layout changes.
    pub const STANDARD_DEFAULT_SPATIAL: Self = Self {
        damping: 0.9,
        stiffness: 700.0,
    };
    /// Standard FastSpatial (ζ=0.9, k=1400) — quick utilitarian transitions.
    pub const STANDARD_FAST_SPATIAL: Self = Self {
        damping: 0.9,
        stiffness: 1400.0,
    };
    /// Standard SlowSpatial (ζ=0.9, k=300) — deliberate layout shifts.
    pub const STANDARD_SLOW_SPATIAL: Self = Self {
        damping: 0.9,
        stiffness: 300.0,
    };
    /// Standard DefaultEffects (ζ=1.0, k=1600) — color/opacity changes.
    pub const STANDARD_DEFAULT_EFFECTS: Self = Self {
        damping: 1.0,
        stiffness: 1600.0,
    };
    /// Standard FastEffects (ζ=1.0, k=3800) — quick state highlights.
    pub const STANDARD_FAST_EFFECTS: Self = Self {
        damping: 1.0,
        stiffness: 3800.0,
    };
    /// Standard SlowEffects (ζ=1.0, k=800) — gradual fades.
    pub const STANDARD_SLOW_EFFECTS: Self = Self {
        damping: 1.0,
        stiffness: 800.0,
    };

    /// Creates a custom spring spec.
    pub const fn new(damping: f32, stiffness: f32) -> Self {
        Self { damping, stiffness }
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
            1.0 - (-alpha * t_seconds).exp()
                * ((omega_d * t_seconds).cos() + beta * (omega_d * t_seconds).sin())
        } else {
            // Overdamped (zeta > 1.0)
            let gamma = (zeta * zeta - 1.0).sqrt();
            let r1 = -omega_n * (zeta - gamma);
            let r2 = -omega_n * (zeta + gamma);
            1.0 - (r2 * (r1 * t_seconds).exp() - r1 * (r2 * t_seconds).exp()) / (r2 - r1)
        }
    }

    /// Returns true if this spring is underdamped (will overshoot).
    pub fn is_bouncy(&self) -> bool {
        self.damping < 1.0
    }
}

// ── Legacy alias ────────────────────────────────────────────────────────────

/// Legacy alias for `SpringSpec`.
///
/// Previous code used `ExpressiveSpring::fast_spatial()` etc. This type alias
/// preserves backward compatibility while the codebase migrates to `SpringSpec`.
pub type ExpressiveSpring = SpringSpec;

/// Backward-compatible constructors matching the original `ExpressiveSpring` API.
impl SpringSpec {
    /// FastSpatial spring spec (damping = 0.6, stiffness = 800.0).
    /// Produces a responsive shape morph with subtle spring bounciness.
    #[inline]
    pub const fn fast_spatial() -> Self {
        Self::EXPRESSIVE_FAST_SPATIAL
    }

    /// DefaultSpatial spring spec (damping = 0.8, stiffness = 380.0).
    #[inline]
    pub const fn default_spatial() -> Self {
        Self::EXPRESSIVE_DEFAULT_SPATIAL
    }

    /// FastEffects spring spec (damping = 1.0, stiffness = 3800.0).
    #[inline]
    pub const fn fast_effects() -> Self {
        Self::EXPRESSIVE_FAST_EFFECTS
    }

    /// SlowSpatial spring spec (damping = 0.8, stiffness = 200.0).
    /// Gentle spring for page transitions and large layout shifts.
    #[inline]
    pub const fn slow_spatial() -> Self {
        Self::EXPRESSIVE_SLOW_SPATIAL
    }

    /// DefaultEffects spring spec (damping = 1.0, stiffness = 1600.0).
    /// Standard color/opacity state transitions.
    #[inline]
    pub const fn default_effects() -> Self {
        Self::EXPRESSIVE_DEFAULT_EFFECTS
    }

    /// SlowEffects spring spec (damping = 1.0, stiffness = 800.0).
    /// Gradual background and ambient transitions.
    #[inline]
    pub const fn slow_effects() -> Self {
        Self::EXPRESSIVE_SLOW_EFFECTS
    }
}

// ── Motion Scheme ───────────────────────────────────────────────────────────

/// Material 3 Motion Scheme — groups 6 spring specs into spatial and effects categories.
///
/// **Spatial specs** drive layout changes, position shifts, scale, and shape morphing.
/// They use underdamped springs (ζ < 1) in the expressive scheme for lively bounce,
/// or near-critically-damped springs (ζ = 0.9) in the standard scheme for subdued motion.
///
/// **Effects specs** drive non-spatial changes like color, opacity, and state highlights.
/// They always use critically damped springs (ζ = 1) with no overshoot.
///
/// # Reference
/// AndroidX `MotionScheme.kt` — `MotionScheme.expressive()` and `MotionScheme.standard()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionScheme {
    /// Default speed spatial spring — standard layout and shape changes.
    pub default_spatial: SpringSpec,
    /// Fast spatial spring — snappy micro-interactions, press morphs, hover effects.
    pub fast_spatial: SpringSpec,
    /// Slow spatial spring — page transitions, large-scale layout shifts.
    pub slow_spatial: SpringSpec,
    /// Default speed effects spring — color/opacity state changes.
    pub default_effects: SpringSpec,
    /// Fast effects spring — hover color changes, ripple fades.
    pub fast_effects: SpringSpec,
    /// Slow effects spring — background transitions, ambient fades.
    pub slow_effects: SpringSpec,
}

impl MotionScheme {
    /// M3 Expressive motion scheme — spirited, lively motion with spring bounce.
    ///
    /// Spatial specs use underdamped springs (ζ = 0.6–0.8) for deliberate overshoot
    /// that conveys physicality and responsiveness. Best for hero moments and
    /// primary UI interactions.
    pub const fn expressive() -> Self {
        Self {
            default_spatial: SpringSpec::EXPRESSIVE_DEFAULT_SPATIAL,
            fast_spatial: SpringSpec::EXPRESSIVE_FAST_SPATIAL,
            slow_spatial: SpringSpec::EXPRESSIVE_SLOW_SPATIAL,
            default_effects: SpringSpec::EXPRESSIVE_DEFAULT_EFFECTS,
            fast_effects: SpringSpec::EXPRESSIVE_FAST_EFFECTS,
            slow_effects: SpringSpec::EXPRESSIVE_SLOW_EFFECTS,
        }
    }

    /// M3 Standard motion scheme — functional, subdued motion without bounce.
    ///
    /// Spatial specs use near-critically-damped springs (ζ = 0.9) with minimal
    /// overshoot. Best for utilitarian, dense-workflow, or productivity applications
    /// where motion should be quick and unobtrusive.
    pub const fn standard() -> Self {
        Self {
            default_spatial: SpringSpec::STANDARD_DEFAULT_SPATIAL,
            fast_spatial: SpringSpec::STANDARD_FAST_SPATIAL,
            slow_spatial: SpringSpec::STANDARD_SLOW_SPATIAL,
            default_effects: SpringSpec::STANDARD_DEFAULT_EFFECTS,
            fast_effects: SpringSpec::STANDARD_FAST_EFFECTS,
            slow_effects: SpringSpec::STANDARD_SLOW_EFFECTS,
        }
    }
}

/// Linearly interpolates between two corner radii specs based on spring `progress`
pub fn lerp_corners(
    start: Corners<Pixels>,
    target: Corners<Pixels>,
    progress: f32,
) -> Corners<Pixels> {
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

    // ── Legacy alias backward compat ────────────────────────────────────

    #[test]
    fn test_expressive_spring_alias() {
        // ExpressiveSpring is a type alias for SpringSpec
        let a: ExpressiveSpring = ExpressiveSpring::fast_spatial();
        let b: SpringSpec = SpringSpec::fast_spatial();
        assert_eq!(a, b);
    }

    // ── Expressive spatial springs (underdamped, bouncy) ────────────────

    #[test]
    fn test_fast_spatial_overshoot() {
        let spring = SpringSpec::EXPRESSIVE_FAST_SPATIAL;
        assert_eq!(spring.damping, 0.6);
        assert_eq!(spring.stiffness, 800.0);
        assert!(spring.is_bouncy());

        assert_eq!(spring.evaluate(0.0), 0.0);

        // Underdamped fast spring should overshoot past 1.0
        let p_over = spring.evaluate(0.15);
        assert!(p_over >= 1.0, "Expected overshoot, got {p_over}");

        // Settles near 1.0
        let p_end = spring.evaluate(0.35);
        assert!((p_end - 1.0).abs() < 0.02, "Expected settled, got {p_end}");
    }

    #[test]
    fn test_default_spatial_underdamped() {
        let spring = SpringSpec::EXPRESSIVE_DEFAULT_SPATIAL;
        assert_eq!(spring.damping, 0.8);
        assert_eq!(spring.stiffness, 380.0);
        assert!(spring.is_bouncy());

        // Should reach near 1.0 and settle
        let p = spring.evaluate(0.5);
        assert!((p - 1.0).abs() < 0.05, "Expected near 1.0, got {p}");
    }

    #[test]
    fn test_slow_spatial_underdamped() {
        let spring = SpringSpec::EXPRESSIVE_SLOW_SPATIAL;
        assert_eq!(spring.damping, 0.8);
        assert_eq!(spring.stiffness, 200.0);
        assert!(spring.is_bouncy());

        // Slower than default — at 0.2s should still be mid-transition
        let p = spring.evaluate(0.2);
        assert!(p > 0.3 && p < 1.2, "Expected mid-range, got {p}");
    }

    // ── Expressive effects springs (critically damped, no bounce) ───────

    #[test]
    fn test_default_effects_critically_damped() {
        let spring = SpringSpec::EXPRESSIVE_DEFAULT_EFFECTS;
        assert_eq!(spring.damping, 1.0);
        assert_eq!(spring.stiffness, 1600.0);
        assert!(!spring.is_bouncy());

        // Critically damped should approach 1.0 monotonically (never exceed)
        let mut prev = 0.0f32;
        for i in 1..=20 {
            let t = i as f32 * 0.01;
            let p = spring.evaluate(t);
            assert!(
                p >= prev - 1e-6,
                "Non-monotonic at t={t}: prev={prev}, p={p}"
            );
            assert!(p <= 1.0 + 1e-6, "Overshoot at t={t}: p={p}");
            prev = p;
        }
    }

    #[test]
    fn test_fast_effects_critically_damped() {
        let spring = SpringSpec::EXPRESSIVE_FAST_EFFECTS;
        assert_eq!(spring.damping, 1.0);
        assert_eq!(spring.stiffness, 3800.0);
        assert!(!spring.is_bouncy());

        // Very fast — should be well past midpoint at 0.05s
        let p = spring.evaluate(0.05);
        assert!(p > 0.75, "Expected past midpoint, got {p}");
    }

    #[test]
    fn test_slow_effects_critically_damped() {
        let spring = SpringSpec::EXPRESSIVE_SLOW_EFFECTS;
        assert_eq!(spring.damping, 1.0);
        assert_eq!(spring.stiffness, 800.0);
        assert!(!spring.is_bouncy());
    }

    // ── Standard spatial springs (near-critically damped, minimal bounce) ─

    #[test]
    fn test_standard_spatial_token_values() {
        assert_eq!(SpringSpec::STANDARD_DEFAULT_SPATIAL.damping, 0.9);
        assert_eq!(SpringSpec::STANDARD_DEFAULT_SPATIAL.stiffness, 700.0);
        assert_eq!(SpringSpec::STANDARD_FAST_SPATIAL.damping, 0.9);
        assert_eq!(SpringSpec::STANDARD_FAST_SPATIAL.stiffness, 1400.0);
        assert_eq!(SpringSpec::STANDARD_SLOW_SPATIAL.damping, 0.9);
        assert_eq!(SpringSpec::STANDARD_SLOW_SPATIAL.stiffness, 300.0);
    }

    #[test]
    fn test_standard_effects_token_values() {
        // Standard and expressive share the same effects tokens
        assert_eq!(
            SpringSpec::STANDARD_DEFAULT_EFFECTS,
            SpringSpec::EXPRESSIVE_DEFAULT_EFFECTS
        );
        assert_eq!(
            SpringSpec::STANDARD_FAST_EFFECTS,
            SpringSpec::EXPRESSIVE_FAST_EFFECTS
        );
        assert_eq!(
            SpringSpec::STANDARD_SLOW_EFFECTS,
            SpringSpec::EXPRESSIVE_SLOW_EFFECTS
        );
    }

    // ── MotionScheme ────────────────────────────────────────────────────

    #[test]
    fn test_motion_scheme_expressive() {
        let scheme = MotionScheme::expressive();
        assert_eq!(
            scheme.default_spatial,
            SpringSpec::EXPRESSIVE_DEFAULT_SPATIAL
        );
        assert_eq!(scheme.fast_spatial, SpringSpec::EXPRESSIVE_FAST_SPATIAL);
        assert_eq!(scheme.slow_spatial, SpringSpec::EXPRESSIVE_SLOW_SPATIAL);
        assert_eq!(
            scheme.default_effects,
            SpringSpec::EXPRESSIVE_DEFAULT_EFFECTS
        );
        assert_eq!(scheme.fast_effects, SpringSpec::EXPRESSIVE_FAST_EFFECTS);
        assert_eq!(scheme.slow_effects, SpringSpec::EXPRESSIVE_SLOW_EFFECTS);
    }

    #[test]
    fn test_motion_scheme_standard() {
        let scheme = MotionScheme::standard();
        assert_eq!(scheme.default_spatial, SpringSpec::STANDARD_DEFAULT_SPATIAL);
        assert_eq!(scheme.fast_spatial, SpringSpec::STANDARD_FAST_SPATIAL);
        assert_eq!(scheme.slow_spatial, SpringSpec::STANDARD_SLOW_SPATIAL);
        assert_eq!(scheme.default_effects, SpringSpec::STANDARD_DEFAULT_EFFECTS);
        assert_eq!(scheme.fast_effects, SpringSpec::STANDARD_FAST_EFFECTS);
        assert_eq!(scheme.slow_effects, SpringSpec::STANDARD_SLOW_EFFECTS);
    }

    #[test]
    fn test_motion_scheme_expressive_spatial_is_bouncy() {
        let scheme = MotionScheme::expressive();
        assert!(scheme.default_spatial.is_bouncy());
        assert!(scheme.fast_spatial.is_bouncy());
        assert!(scheme.slow_spatial.is_bouncy());
        assert!(!scheme.default_effects.is_bouncy());
        assert!(!scheme.fast_effects.is_bouncy());
        assert!(!scheme.slow_effects.is_bouncy());
    }

    // ── Backward-compat named constructors ──────────────────────────────

    #[test]
    fn test_backward_compat_constructors() {
        assert_eq!(
            SpringSpec::fast_spatial(),
            SpringSpec::EXPRESSIVE_FAST_SPATIAL
        );
        assert_eq!(
            SpringSpec::default_spatial(),
            SpringSpec::EXPRESSIVE_DEFAULT_SPATIAL
        );
        assert_eq!(
            SpringSpec::fast_effects(),
            SpringSpec::EXPRESSIVE_FAST_EFFECTS
        );
        assert_eq!(
            SpringSpec::slow_spatial(),
            SpringSpec::EXPRESSIVE_SLOW_SPATIAL
        );
        assert_eq!(
            SpringSpec::default_effects(),
            SpringSpec::EXPRESSIVE_DEFAULT_EFFECTS
        );
        assert_eq!(
            SpringSpec::slow_effects(),
            SpringSpec::EXPRESSIVE_SLOW_EFFECTS
        );
    }

    // ── Corner lerp ─────────────────────────────────────────────────────

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

    // ── Custom spring ───────────────────────────────────────────────────

    #[test]
    fn test_custom_spring() {
        let spring = SpringSpec::new(0.7, 500.0);
        assert_eq!(spring.damping, 0.7);
        assert_eq!(spring.stiffness, 500.0);
        assert!(spring.is_bouncy());
    }
}
