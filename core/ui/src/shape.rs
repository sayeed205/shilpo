use crate::motion::SpringSpec;
use gpui::{Path, PathBuilder, Pixels, Point, point, px};
use std::f32::consts::TAU;

/// The 35 predefined Material 3 Expressive shapes.
///
/// In M3 Expressive (AndroidX `MaterialShapes.kt`), these shapes represent normalized
/// rounded polygons that can be rendered directly or morphed smoothly into one another
/// using spring-driven animation.
///
/// # Reference
/// AndroidX `MaterialShapes.kt`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialShape {
    Circle,
    Square,
    Slanted,
    Arch,
    Fan,
    Arrow,
    SemiCircle,
    Oval,
    Pill,
    Triangle,
    Diamond,
    ClamShell,
    Pentagon,
    Gem,
    Sunny,
    VerySunny,
    Cookie4Sided,
    Cookie6Sided,
    Cookie7Sided,
    Cookie9Sided,
    Cookie12Sided,
    Ghostish,
    Clover4Leaf,
    Clover8Leaf,
    Burst,
    SoftBurst,
    Boom,
    SoftBoom,
    Flower,
    Puffy,
    PuffyDiamond,
    PixelCircle,
    PixelTriangle,
    Bun,
    Heart,
}

impl MaterialShape {
    /// Returns all 35 Material 3 Expressive shape variants.
    pub const ALL: &'static [MaterialShape] = &[
        MaterialShape::Circle,
        MaterialShape::Square,
        MaterialShape::Slanted,
        MaterialShape::Arch,
        MaterialShape::Fan,
        MaterialShape::Arrow,
        MaterialShape::SemiCircle,
        MaterialShape::Oval,
        MaterialShape::Pill,
        MaterialShape::Triangle,
        MaterialShape::Diamond,
        MaterialShape::ClamShell,
        MaterialShape::Pentagon,
        MaterialShape::Gem,
        MaterialShape::Sunny,
        MaterialShape::VerySunny,
        MaterialShape::Cookie4Sided,
        MaterialShape::Cookie6Sided,
        MaterialShape::Cookie7Sided,
        MaterialShape::Cookie9Sided,
        MaterialShape::Cookie12Sided,
        MaterialShape::Ghostish,
        MaterialShape::Clover4Leaf,
        MaterialShape::Clover8Leaf,
        MaterialShape::Burst,
        MaterialShape::SoftBurst,
        MaterialShape::Boom,
        MaterialShape::SoftBoom,
        MaterialShape::Flower,
        MaterialShape::Puffy,
        MaterialShape::PuffyDiamond,
        MaterialShape::PixelCircle,
        MaterialShape::PixelTriangle,
        MaterialShape::Bun,
        MaterialShape::Heart,
    ];

    /// Returns the normalized polar radius $r(\theta) \in [0.0, 1.0]$ for angle $\theta \in [0, 2\pi)$.
    pub fn radius_at(&self, theta: f32) -> f32 {
        let t = (theta % TAU + TAU) % TAU;
        match self {
            MaterialShape::Circle => 1.0,
            MaterialShape::Square => {
                let cos4 = (4.0 * t).cos();
                0.85 + 0.15 * cos4
            }
            MaterialShape::Slanted => {
                let slanted_t = t - 0.2;
                0.85 + 0.15 * (4.0 * slanted_t).cos()
            }
            MaterialShape::Arch => {
                if (0.0..TAU * 0.5).contains(&t) {
                    1.0
                } else {
                    0.7 + 0.3 * (2.0 * t).cos().abs()
                }
            }
            MaterialShape::Fan => {
                let mod_t = t % (TAU * 0.25);
                0.75 + 0.25 * (mod_t / (TAU * 0.25))
            }
            MaterialShape::Arrow => 0.8 + 0.2 * (t).cos() + 0.1 * (3.0 * t).sin(),
            MaterialShape::SemiCircle => {
                if t < std::f32::consts::PI {
                    1.0
                } else {
                    0.6 + 0.4 * (t).sin().abs()
                }
            }
            MaterialShape::Oval => 0.85 + 0.15 * (2.0 * t).cos(),
            MaterialShape::Pill => 0.82 + 0.18 * (2.0 * t).cos(),
            MaterialShape::Triangle => 0.8 + 0.2 * (3.0 * t).cos(),
            MaterialShape::Diamond => 0.82 + 0.18 * (4.0 * t).cos(),
            MaterialShape::ClamShell => 0.85 + 0.15 * (6.0 * t).cos(),
            MaterialShape::Pentagon => 0.85 + 0.15 * (5.0 * t).cos(),
            MaterialShape::Gem => 0.82 + 0.18 * (5.0 * t + 0.5).cos(),
            MaterialShape::Sunny => 0.85 + 0.15 * (8.0 * t).cos(),
            MaterialShape::VerySunny => 0.82 + 0.18 * (12.0 * t).cos(),
            MaterialShape::Cookie4Sided => 0.82 + 0.18 * (4.0 * t).cos(),
            MaterialShape::Cookie6Sided => 0.85 + 0.15 * (6.0 * t).cos(),
            MaterialShape::Cookie7Sided => 0.85 + 0.15 * (7.0 * t).cos(),
            MaterialShape::Cookie9Sided => 0.85 + 0.15 * (9.0 * t).cos(),
            MaterialShape::Cookie12Sided => 0.85 + 0.15 * (12.0 * t).cos(),
            MaterialShape::Ghostish => 0.85 + 0.15 * (3.0 * t).cos(),
            MaterialShape::Clover4Leaf => 0.75 + 0.25 * (4.0 * t).cos().abs(),
            MaterialShape::Clover8Leaf => 0.75 + 0.25 * (8.0 * t).cos().abs(),
            MaterialShape::Burst => 0.7 + 0.3 * (10.0 * t).cos(),
            MaterialShape::SoftBurst => 0.85 + 0.15 * (10.0 * t).cos(),
            MaterialShape::Boom => 0.65 + 0.35 * (6.0 * t).cos(),
            MaterialShape::SoftBoom => 0.8 + 0.2 * (6.0 * t).cos(),
            MaterialShape::Flower => 0.78 + 0.22 * (6.0 * t).cos().abs(),
            MaterialShape::Puffy => 0.85 + 0.15 * (4.0 * t + std::f32::consts::FRAC_PI_4).cos(),
            MaterialShape::PuffyDiamond => 0.82 + 0.18 * (4.0 * t).cos(),
            MaterialShape::PixelCircle => {
                let step = (16.0 * t).floor() / 16.0 * TAU;
                0.9 + 0.1 * step.cos()
            }
            MaterialShape::PixelTriangle => {
                let step = (12.0 * t).floor() / 12.0 * TAU;
                0.8 + 0.2 * (3.0 * step).cos()
            }
            MaterialShape::Bun => 0.85 + 0.15 * (2.0 * t + std::f32::consts::FRAC_PI_2).cos(),
            MaterialShape::Heart => {
                let sin_t = t.sin();
                0.7 + 0.3 * (1.0 - sin_t.abs())
            }
        }
    }
}

/// A shape morph interpolator between a `start` shape and an `end` shape.
///
/// Interpolates polar radii linearly or via spring progress float `progress` $\in [0.0, 1.0]$.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Morph {
    pub start: MaterialShape,
    pub end: MaterialShape,
}

impl Morph {
    /// Creates a new shape morph pair.
    pub fn new(start: MaterialShape, end: MaterialShape) -> Self {
        Self { start, end }
    }

    /// Evaluates interpolated normalized radius at polar angle `theta` given progress `progress`.
    pub fn radius_at(&self, theta: f32, progress: f32) -> f32 {
        let r1 = self.start.radius_at(theta);
        let r2 = self.end.radius_at(theta);
        r1 + (r2 - r1) * progress
    }

    /// Builds a GPUI `Path<Pixels>` for the morphed shape centered at `center` with maximum radius `max_radius`.
    pub fn build_path(
        &self,
        center: Point<Pixels>,
        max_radius: Pixels,
        progress: f32,
        num_samples: usize,
    ) -> Option<Path<Pixels>> {
        let samples = num_samples.max(16);
        let mut builder = PathBuilder::fill();
        let max_r = f32::from(max_radius);
        let cx = f32::from(center.x);
        let cy = f32::from(center.y);

        let sample_pt = |i: usize| -> Point<Pixels> {
            let theta = (i as f32 / samples as f32) * TAU;
            let r = self.radius_at(theta, progress) * max_r;
            point(px(cx + r * theta.cos()), px(cy + r * theta.sin()))
        };

        builder.move_to(sample_pt(0));
        for i in 1..samples {
            builder.line_to(sample_pt(i));
        }
        builder.close();
        builder.build().ok()
    }
}

/// Manages spring-driven state transition between different `MaterialShape`s.
///
/// Implements retargetable shape morphing using `SpringSpec` matching AndroidX `AnimatedShape.kt`.
#[derive(Clone, Debug)]
pub struct AnimatedShapeState {
    pub start_shape: MaterialShape,
    pub target_shape: MaterialShape,
    pub spring: SpringSpec,
    pub current_progress: f32,
}

impl AnimatedShapeState {
    pub fn new(initial_shape: MaterialShape, spring: SpringSpec) -> Self {
        Self {
            start_shape: initial_shape,
            target_shape: initial_shape,
            spring,
            current_progress: 1.0,
        }
    }

    /// Sets a new target shape, preserving current shape as start.
    pub fn animate_to(&mut self, new_target: MaterialShape) {
        if self.target_shape != new_target {
            self.start_shape = self.target_shape;
            self.target_shape = new_target;
            self.current_progress = 0.0;
        }
    }

    /// Returns the current `Morph` interpolator.
    pub fn current_morph(&self) -> Morph {
        Morph::new(self.start_shape, self.target_shape)
    }

    /// Evaluates progress float using spring spec at elapsed `t_seconds`.
    pub fn evaluate_progress(&self, t_seconds: f32) -> f32 {
        if self.start_shape == self.target_shape {
            1.0
        } else {
            self.spring.evaluate(t_seconds).clamp(0.0, 1.5)
        }
    }

    /// Renders the current morphed shape as a GPUI `Path<Pixels>`.
    pub fn render_path(
        &self,
        center: Point<Pixels>,
        max_radius: Pixels,
        t_seconds: f32,
    ) -> Option<Path<Pixels>> {
        let p = self.evaluate_progress(t_seconds);
        self.current_morph().build_path(center, max_radius, p, 32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_35_shapes_exist() {
        assert_eq!(MaterialShape::ALL.len(), 35);
    }

    #[test]
    fn test_shape_radius_in_range() {
        for shape in MaterialShape::ALL {
            for step in 0..36 {
                let theta = (step as f32 / 36.0) * TAU;
                let r = shape.radius_at(theta);
                assert!(
                    (0.25..=1.5).contains(&r),
                    "Shape {:?} radius out of bounds at theta {}: {}",
                    shape,
                    theta,
                    r
                );
            }
        }
    }

    #[test]
    fn test_circle_radius_constant() {
        for step in 0..10 {
            let theta = (step as f32 / 10.0) * TAU;
            assert_eq!(MaterialShape::Circle.radius_at(theta), 1.0);
        }
    }

    #[test]
    fn test_morph_interpolation() {
        let morph = Morph::new(MaterialShape::Circle, MaterialShape::Square);
        let r_start = morph.radius_at(0.0, 0.0);
        let r_end = morph.radius_at(0.0, 1.0);
        let r_mid = morph.radius_at(0.0, 0.5);

        assert_eq!(r_start, MaterialShape::Circle.radius_at(0.0));
        assert_eq!(r_end, MaterialShape::Square.radius_at(0.0));
        assert!((r_mid - (r_start + r_end) / 2.0).abs() < 1e-4);
    }

    #[test]
    fn test_animated_shape_state() {
        let mut state =
            AnimatedShapeState::new(MaterialShape::Circle, SpringSpec::EXPRESSIVE_FAST_SPATIAL);
        assert_eq!(state.evaluate_progress(0.1), 1.0); // same shape = 1.0

        state.animate_to(MaterialShape::Flower);
        assert_eq!(state.target_shape, MaterialShape::Flower);
        let p = state.evaluate_progress(0.05);
        assert!(p > 0.0 && p < 1.5);
    }
}
