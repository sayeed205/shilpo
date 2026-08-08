use std::thread;
use std::time::{Duration, Instant};

/// Sleep-based frame rate limiter to throttle video frame generation
pub struct FpsLimiter {
    target_interval: Duration,
    last_tick: Instant,
}

impl FpsLimiter {
    pub fn new(target_fps: u32) -> Self {
        let fps = target_fps.max(1);
        Self {
            target_interval: Duration::from_micros(1_000_000 / fps as u64),
            last_tick: Instant::now(),
        }
    }

    pub fn tick(&mut self) {
        let elapsed = self.last_tick.elapsed();
        if elapsed < self.target_interval {
            thread::sleep(self.target_interval - elapsed);
        }
        self.last_tick = Instant::now();
    }
}
