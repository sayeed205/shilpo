use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;

use crate::pipeline::RecordingPipeline;
use crate::types::{RecordingEvent, RecordingRequest, RecordingState, StreamConfig};

struct Inner {
    state: RecordingState,
    pipeline: Option<RecordingPipeline>,
    start_time: Option<Instant>,
    paused_duration: Duration,
    pause_started: Option<Instant>,
}

#[derive(Clone)]
pub struct RecordingController {
    inner: Arc<Mutex<Inner>>,
    event_tx: Sender<RecordingEvent>,
    event_rx: Receiver<RecordingEvent>,
}

impl RecordingController {
    pub fn new() -> Self {
        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state: RecordingState::Idle,
                pipeline: None,
                start_time: None,
                paused_duration: Duration::ZERO,
                pause_started: None,
            })),
            event_tx,
            event_rx,
        }
    }

    pub fn start(&self, request: RecordingRequest, config: StreamConfig) -> anyhow::Result<()> {
        let mut inner = self.inner.lock();
        if !matches!(
            inner.state,
            RecordingState::Idle | RecordingState::Selecting
        ) {
            anyhow::bail!("Recording session already active");
        }

        let pipeline = RecordingPipeline::start(request, config, self.event_tx.clone())?;
        inner.pipeline = Some(pipeline);
        inner.start_time = Some(Instant::now());
        inner.paused_duration = Duration::ZERO;
        inner.pause_started = None;
        inner.state = RecordingState::Recording {
            elapsed: Duration::ZERO,
        };

        let _ = self
            .event_tx
            .send(RecordingEvent::StateChanged(inner.state));
        Ok(())
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let mut inner = self.inner.lock();
        if matches!(inner.state, RecordingState::Idle) {
            return Ok(());
        }

        inner.state = RecordingState::Finalizing;
        let _ = self
            .event_tx
            .send(RecordingEvent::StateChanged(inner.state));

        let result = if let Some(mut pipeline) = inner.pipeline.take() {
            pipeline.stop().map(|_| ())
        } else {
            Ok(())
        };

        if let Err(error) = &result {
            let _ = self.event_tx.send(RecordingEvent::Error(error.to_string()));
            inner.state = RecordingState::Idle;
            inner.start_time = None;
            inner.paused_duration = Duration::ZERO;
            inner.pause_started = None;
            let _ = self
                .event_tx
                .send(RecordingEvent::StateChanged(inner.state));
        } else {
            inner.state = RecordingState::Idle;
            inner.start_time = None;
            inner.paused_duration = Duration::ZERO;
            inner.pause_started = None;
            let _ = self
                .event_tx
                .send(RecordingEvent::StateChanged(inner.state));
        }
        result
    }

    pub fn pause(&self) -> anyhow::Result<()> {
        let mut inner = self.inner.lock();
        if let RecordingState::Recording { elapsed } = inner.state {
            if let Some(pipeline) = inner.pipeline.as_ref() {
                pipeline.pause();
            }
            let elapsed = inner
                .start_time
                .map(|start| start.elapsed().saturating_sub(inner.paused_duration))
                .unwrap_or(elapsed);
            inner.pause_started = Some(Instant::now());
            inner.state = RecordingState::Paused { elapsed };
            let _ = self
                .event_tx
                .send(RecordingEvent::StateChanged(inner.state));
        }
        Ok(())
    }

    pub fn resume(&self) -> anyhow::Result<()> {
        let mut inner = self.inner.lock();
        if let RecordingState::Paused { elapsed } = inner.state {
            if let Some(pipeline) = inner.pipeline.as_ref() {
                pipeline.resume();
            }
            if let Some(started) = inner.pause_started.take() {
                inner.paused_duration += started.elapsed();
            }
            let elapsed = inner
                .start_time
                .map(|start| start.elapsed().saturating_sub(inner.paused_duration))
                .unwrap_or(elapsed);
            inner.state = RecordingState::Recording { elapsed };
            let _ = self
                .event_tx
                .send(RecordingEvent::StateChanged(inner.state));
        }
        Ok(())
    }

    pub fn cancel(&self) -> anyhow::Result<()> {
        let mut inner = self.inner.lock();
        if !inner.state.is_active() {
            return Ok(());
        }
        if let Some(mut pipeline) = inner.pipeline.take() {
            let _ = pipeline.cancel();
        }
        inner.start_time = None;
        inner.paused_duration = Duration::ZERO;
        inner.pause_started = None;
        inner.state = RecordingState::Idle;
        let _ = self
            .event_tx
            .send(RecordingEvent::StateChanged(inner.state));
        Ok(())
    }

    pub fn state(&self) -> RecordingState {
        let mut inner = self.inner.lock();
        if let (RecordingState::Recording { .. }, Some(start)) = (inner.state, inner.start_time) {
            inner.state = RecordingState::Recording {
                elapsed: start.elapsed().saturating_sub(inner.paused_duration),
            };
        }
        inner.state
    }

    pub fn events(&self) -> &Receiver<RecordingEvent> {
        &self.event_rx
    }

    pub fn shutdown(&self) {
        let _ = self.stop();
    }
}

impl Default for RecordingController {
    fn default() -> Self {
        Self::new()
    }
}
