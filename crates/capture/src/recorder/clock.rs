use std::time::{Duration, Instant};

/// Monotonic media clock that excludes time spent paused.
#[derive(Debug, Default)]
pub struct SessionClock {
    started_at: Option<Instant>,
    paused_at: Option<Instant>,
    paused_total: Duration,
    elapsed: Duration,
}

impl SessionClock {
    pub fn start(&mut self) {
        self.started_at = Some(Instant::now());
        self.paused_at = None;
        self.paused_total = Duration::ZERO;
        self.elapsed = Duration::ZERO;
    }

    pub fn pause(&mut self) -> bool {
        if self.started_at.is_none() || self.paused_at.is_some() {
            return false;
        }
        self.paused_at = Some(Instant::now());
        self.refresh();
        true
    }

    pub fn resume(&mut self) -> bool {
        let Some(paused_at) = self.paused_at.take() else {
            return false;
        };
        self.paused_total = self.paused_total.saturating_add(paused_at.elapsed());
        self.refresh();
        true
    }

    pub fn elapsed(&mut self) -> Duration {
        self.refresh();
        self.elapsed
    }

    pub fn paused_total(&self) -> Duration {
        self.paused_total
    }

    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    fn refresh(&mut self) {
        let Some(started_at) = self.started_at else {
            self.elapsed = Duration::ZERO;
            return;
        };
        let current_pause = self.paused_at.map_or(Duration::ZERO, |at| at.elapsed());
        self.elapsed = started_at
            .elapsed()
            .saturating_sub(self.paused_total)
            .saturating_sub(current_pause);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_and_resume_transitions_are_idempotent() {
        let mut clock = SessionClock::default();
        assert!(!clock.pause());
        clock.start();
        assert!(clock.pause());
        assert!(!clock.pause());
        assert!(clock.is_paused());
        assert!(clock.resume());
        assert!(!clock.resume());
        assert!(!clock.is_paused());
    }
}
