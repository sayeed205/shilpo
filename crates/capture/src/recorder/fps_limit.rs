//! Frame-rate limiter that buffers one frame to make better drop decisions.
//!
//! Adapted from `wl-screenrec` (`src/fps_limit.rs`), licensed under the
//! Apache License, Version 2.0. Copyright (c) wl-screenrec contributors.
//! This file is a derived work distributed under the Apache-2.0 license.

use std::time::Duration;

pub struct FpsLimit<T> {
    min_dt: Duration,
    on_deck: Option<(Duration, T)>,
    next_target_time: Option<Duration>,
    dropped_frames: u64,
}

// fps limit for VRR is pretty tricky. We can't just discard frames with close timestamps, because imagine the situation
// where we get the following stream of timestamps (in ms)
// 0, 16, 17, 10000
// we obviously want to drop the 16, not the 17, because that 17 is displayed for a very long time.
// so, basically, we need to add a frame of latency and buffer a frame to know if we should skip a frame
impl<T> FpsLimit<T> {
    pub fn new(max_fps: f64) -> Self {
        assert_ne!(max_fps, 0.);
        Self {
            min_dt: Duration::from_secs_f64(1. / max_fps),
            on_deck: None,
            next_target_time: None,
            dropped_frames: 0,
        }
    }

    pub fn on_new_frame(&mut self, f: T, ts: Duration) -> Option<T> {
        // always send the first frame, could be a long gap after.
        if self.next_target_time.is_none() {
            self.next_target_time = Some(ts + self.min_dt);
            return Some(f);
        }

        // don't have enough info to make a decision, hold on...
        if self.on_deck.is_none() {
            self.on_deck = Some((ts, f));
            return None;
        }

        let (old_ts, old_t) = self.on_deck.take().unwrap();
        let next_target_time = self.next_target_time.unwrap();
        self.on_deck = Some((ts, f));

        if ts < next_target_time {
            // drop
            self.dropped_frames = self.dropped_frames.saturating_add(1);
            None
        } else {
            // max to handle skips better
            self.next_target_time = Some(next_target_time.max(old_ts) + self.min_dt);
            Some(old_t)
        }
    }

    pub fn flush(&mut self) -> Option<T> {
        self.on_deck.take().map(|(_, t)| t)
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    pub fn discarded_frames(&self) -> u64 {
        self.dropped_frames + u64::from(self.on_deck.is_some())
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::FpsLimit;

    #[test]
    fn basic() {
        let mut l = FpsLimit::<u32>::new(1.);
        let s = Duration::from_secs_f32;

        let out_frames: Vec<_> = [
            l.on_new_frame(0, s(0.)),
            l.on_new_frame(1, s(0.5)),
            l.on_new_frame(2, s(1.1)),
            l.on_new_frame(3, s(1.2)),
            l.on_new_frame(4, s(1.3)),
            l.on_new_frame(5, s(5.)),
            l.flush(),
        ]
        .into_iter()
        .flatten()
        .collect();

        assert_eq!(out_frames, [0, 1, 4, 5]);
        assert_eq!(l.dropped_frames(), 2);
    }

    #[test]
    fn synthetic_120hz() {
        let mut l = FpsLimit::<u32>::new(30.);

        let mut acc = vec![];
        for i in 0..120 {
            if let Some(r) = l.on_new_frame(i, Duration::from_micros((i * 1_000_000 / 120) as u64))
            {
                acc.push(r);
            }
        }

        if let Some(r) = l.flush() {
            acc.push(r);
        }

        let ct = acc.len();
        assert!((28..32).contains(&ct), "ct={ct} acc={acc:?}");
    }

    #[test]
    fn large_skip() {
        let mut l = FpsLimit::<u32>::new(1.);
        let s = Duration::from_secs_f32;

        let out_frames: Vec<_> = [
            l.on_new_frame(0, s(0.)),
            l.on_new_frame(1, s(0.5)),
            l.on_new_frame(2, s(10.0)),
            l.on_new_frame(3, s(10.1)),
            l.on_new_frame(4, s(10.2)),
            l.on_new_frame(5, s(10.3)),
            l.flush(),
        ]
        .into_iter()
        .flatten()
        .collect();

        assert_eq!(out_frames, [0, 1, 2, 5])
    }
}
