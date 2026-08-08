use std::time::{Duration, Instant};

/// Monotonic clock for audio/video synchronization across pipeline stages
#[derive(Debug)]
pub struct PipelineClock {
    start_time: Instant,
}

impl PipelineClock {
    pub fn start() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn pts(&self, time_base_den: u32) -> i64 {
        let secs = self.elapsed().as_secs_f64();
        (secs * time_base_den as f64) as i64
    }
}
