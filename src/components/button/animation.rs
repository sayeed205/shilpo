use std::sync::Mutex;
use std::time::Instant;

use crate::ui::motion;

// Material 3 Expressive DefaultEffects: damping ratio 1.0, stiffness 1600.
const STIFFNESS: f32 = 1600.0;
const PRECISION: f32 = 0.01;
const MAX_STEP: f32 = 1.0 / 30.0;

pub(super) struct Spring {
    from: f32,
    velocity: f32,
    target: f32,
    clock: Mutex<Clock>,
}

struct Clock {
    elapsed: f32,
    last: Instant,
}

impl Clock {
    fn new_at(now: Instant) -> Self {
        Self {
            elapsed: 0.0,
            last: now,
        }
    }

    fn advance(&mut self, now: Instant, speed: f32) -> f32 {
        let step = now.duration_since(self.last).as_secs_f32().min(MAX_STEP) * speed;

        self.elapsed += step;
        self.last = now;

        self.elapsed
    }
}

impl Spring {
    pub(super) fn new_at(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            velocity: 0.0,
            target: value,
            clock: Mutex::new(Clock::new_at(now)),
        }
    }

    pub(super) fn to_at(&mut self, target: f32, now: Instant, speed: f32) {
        if target == self.target {
            return;
        }

        let clock = self
            .clock
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let elapsed = clock.advance(now, speed);
        let (position, velocity) = critical(self.from, self.velocity, self.target, elapsed);

        // Carry position and velocity into the reversed spring, so a quick release never snaps.
        self.from = position;
        self.velocity = velocity;
        self.target = target;
        *clock = Clock::new_at(now);
    }

    #[cfg(test)]
    pub(super) fn target(&self) -> f32 {
        self.target
    }

    pub(super) fn value(&self) -> f32 {
        if motion::reduced() {
            return self.target;
        }

        let now = Instant::now();
        let speed = motion::speed();
        let mut clock = self
            .clock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let elapsed = clock.advance(now, speed);
        let (position, velocity) = critical(self.from, self.velocity, self.target, elapsed);

        if (position - self.target).abs() <= PRECISION && velocity.abs() <= PRECISION {
            return self.target;
        }

        amane::request_frame();

        position
    }
}

// Exact critically damped response for stiffness 1600 (natural frequency sqrt(k) = 40).
fn critical(from: f32, velocity: f32, target: f32, time: f32) -> (f32, f32) {
    let natural = STIFFNESS.sqrt();
    let offset = from - target;
    let second = velocity + natural * offset;
    let decay = (-natural * time).exp();
    let wave = offset + second * time;

    let position = target + wave * decay;
    let velocity = (second - natural * wave) * decay;

    (position, velocity)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{STIFFNESS, Spring, critical};

    fn near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
    }

    #[test]
    fn critical_spring_starts_at_the_current_corner_radius() {
        let (position, velocity) = critical(20.0, 0.0, 8.0, 0.0);

        near(position, 20.0);
        near(velocity, 0.0);
    }

    #[test]
    fn default_effects_spring_matches_the_analytical_critical_solution() {
        let time = 1.0 / STIFFNESS.sqrt();
        let (position, velocity) = critical(20.0, 0.0, 8.0, time);

        // At one natural time constant: x = 8 + 24/e and v = -480/e.
        near(position, 8.0 + 24.0 / std::f32::consts::E);
        near(velocity, -480.0 / std::f32::consts::E);
    }

    #[test]
    fn reversing_a_morph_keeps_position_and_velocity_continuous() {
        let (before_reverse, speed_before) = critical(20.0, 0.0, 8.0, 0.025);
        let (at_reverse, speed_at_reverse) = critical(before_reverse, speed_before, 20.0, 0.0);

        near(at_reverse, before_reverse);
        near(speed_at_reverse, speed_before);
    }

    #[test]
    fn explicit_time_retargeting_carries_the_current_position_and_velocity() {
        let start = Instant::now();
        let retarget = start + Duration::from_millis(25);
        let mut spring = Spring::new_at(20.0, start);

        spring.to_at(8.0, start, 1.0);
        spring.to_at(20.0, retarget, 1.0);
        let (position, velocity) = critical(20.0, 0.0, 8.0, 0.025);
        near(spring.from, position);
        near(spring.velocity, velocity);
        assert_eq!(spring.target, 20.0);
    }

    #[test]
    fn a_settled_shape_stays_at_its_target() {
        let (position, velocity) = critical(8.0, 0.0, 8.0, 0.5);

        near(position, 8.0);
        near(velocity, 0.0);
    }
}
