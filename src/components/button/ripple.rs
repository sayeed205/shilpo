use std::sync::Mutex;
use std::time::Instant;

use amane::Point;

// Material 3's default bounded ripple timing and geometry.
const FADE_IN_SECONDS: f32 = 0.075;
const EXPANSION_SECONDS: f32 = 0.225;
const FADE_OUT_SECONDS: f32 = 0.150;
const BOUNDED_EXTRA_RADIUS: f32 = 10.0;
const COMPLETION_EPSILON_SECONDS: f32 = 0.000_01;

pub(super) struct Animation {
    origin: Point,
    width: f32,
    height: f32,
    clock: Mutex<Clock>,
    finished_at: Option<f32>,
}

struct Clock {
    elapsed: f32,
    last: Instant,
}

#[derive(Clone, Copy)]
pub(super) struct Frame {
    pub(super) center_x: f32,
    pub(super) center_y: f32,
    pub(super) radius: f32,
    pub(super) alpha: f32,
}

impl Clock {
    fn new_at(now: Instant) -> Self {
        Self {
            elapsed: 0.0,
            last: now,
        }
    }

    fn advance(&mut self, now: Instant, speed: f32) -> f32 {
        self.elapsed += now.saturating_duration_since(self.last).as_secs_f32() * speed;
        self.last = now;
        self.elapsed
    }
}

impl Animation {
    pub(super) fn new_at(origin: Point, width: f32, height: f32, now: Instant) -> Self {
        Self {
            origin,
            width,
            height,
            clock: Mutex::new(Clock::new_at(now)),
            finished_at: None,
        }
    }

    pub(super) fn finish(&mut self, now: Instant, speed: f32) {
        if self.finished_at.is_some() {
            return;
        }

        let clock = self
            .clock
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.finished_at = Some(clock.advance(now, speed));
    }

    pub(super) fn frame(&self, now: Instant, speed: f32) -> Option<Frame> {
        let elapsed = self
            .clock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .advance(now, speed);

        let fade_out_start = self.finished_at.map(|finished_at| {
            f32::max(finished_at, EXPANSION_SECONDS)
        });

        if fade_out_start.is_some_and(|start| {
            elapsed + COMPLETION_EPSILON_SECONDS >= start + FADE_OUT_SECONDS
        }) {
            return None;
        }

        let alpha = match fade_out_start {
            Some(start) if elapsed >= start => {
                (1.0 - (elapsed - start) / FADE_OUT_SECONDS).clamp(0.0, 1.0)
            }
            Some(_) => 1.0,
            None => (elapsed / FADE_IN_SECONDS).min(1.0),
        };

        let progress = (elapsed / EXPANSION_SECONDS).clamp(0.0, 1.0);
        let eased_radius = fast_out_slow_in(progress);
        let start_radius = self.width.max(self.height) * 0.3;
        let end_radius = self.width.hypot(self.height) / 2.0 + BOUNDED_EXTRA_RADIUS;

        Some(Frame {
            center_x: lerp(self.origin.x, self.width / 2.0, progress),
            center_y: lerp(self.origin.y, self.height / 2.0, progress),
            radius: lerp(start_radius, end_radius, eased_radius),
            alpha,
        })
    }
}

fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

// Compose's FastOutSlowInEasing is cubic Bezier (0.4, 0, 0.2, 1).
fn fast_out_slow_in(progress: f32) -> f32 {
    let mut low = 0.0;
    let mut high = 1.0;

    // Solve the Bezier x-coordinate for the requested time, then read its y-coordinate.
    for _ in 0..16 {
        let parameter = (low + high) / 2.0;

        if bezier(parameter, 0.4, 0.2) < progress {
            low = parameter;
        } else {
            high = parameter;
        }
    }

    bezier((low + high) / 2.0, 0.0, 1.0)
}

fn bezier(parameter: f32, first_control: f32, second_control: f32) -> f32 {
    let remaining = 1.0 - parameter;

    3.0 * remaining * remaining * parameter * first_control
        + 3.0 * remaining * parameter * parameter * second_control
        + parameter * parameter * parameter
}
