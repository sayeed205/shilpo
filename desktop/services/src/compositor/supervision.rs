//! Shared compositor domain supervision helpers.
//!
//! Provides common supervision lifecycle transitions, snapshot observation,
//! reconnect backoff, and quarantine management for compositor backends.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use tokio::sync::watch;

use super::{
    CompositorCapabilities, CompositorCommandBroker, CompositorSnapshot, DomainLifecycle,
    DomainVersion, StaleUpdateError, SupervisorState,
};
use crate::domain::DomainSupervisor;

/// Trait providing lifecycle-dependent compositor capability derivation.
pub trait CapabilityProvider: Send + Sync {
    fn capabilities_for(&self, lifecycle: DomainLifecycle) -> CompositorCapabilities;
}

impl<F> CapabilityProvider for F
where
    F: Fn(DomainLifecycle) -> CompositorCapabilities + Send + Sync,
{
    fn capabilities_for(&self, lifecycle: DomainLifecycle) -> CompositorCapabilities {
        self(lifecycle)
    }
}

/// Publishes a reconnecting snapshot through the broker.
pub fn publish_reconnecting<C: CapabilityProvider + ?Sized>(
    tx: &watch::Sender<Arc<CompositorSnapshot>>,
    broker: &CompositorCommandBroker,
    owner_generation: u64,
    revision: &mut u64,
    last_error: Option<String>,
    capabilities: &C,
) {
    let previous = tx.borrow().clone();
    let mut current = (*previous).clone();
    *revision = revision.saturating_add(1);
    current.version = DomainVersion::new(owner_generation, *revision);
    current.connection = DomainLifecycle::Reconnecting;
    current.capabilities = capabilities.capabilities_for(DomainLifecycle::Reconnecting);
    current.last_error = last_error;
    let snap_arc = Arc::new(current);
    if broker.observe_snapshot(snap_arc.clone()).is_ok() {
        let _ = tx.send(snap_arc);
    }
}

/// Records a supervisor failure, transitioning lifecycle to Reconnecting or Unavailable (quarantine),
/// incrementing the snapshot revision, and publishing an updated snapshot through the broker.
#[allow(clippy::too_many_arguments)]
pub fn record_supervisor_failure<C: CapabilityProvider + ?Sized>(
    supervisor: &Arc<Mutex<DomainSupervisor>>,
    broker: &Arc<CompositorCommandBroker>,
    tx: &watch::Sender<Arc<CompositorSnapshot>>,
    owner_generation: u64,
    revision: &mut u64,
    error: String,
    now_ms: u64,
    capabilities: &C,
) {
    let new_state = supervisor.lock().unwrap().record_failure(now_ms);
    let (target_lifecycle, error_msg) = match new_state {
        SupervisorState::Quarantined => {
            tracing::warn!(target: "shilpo_profile", lifecycle = "quarantined", "compositor supervisor transition");
            (
                DomainLifecycle::Unavailable,
                "Quarantined after five failures in 60s".to_string(),
            )
        }
        SupervisorState::Backoff { attempt, .. } => {
            tracing::info!(target: "shilpo_profile", lifecycle = "backoff", attempt, "compositor supervisor transition");
            (DomainLifecycle::Reconnecting, error)
        }
        _ => (DomainLifecycle::Reconnecting, error),
    };

    if new_state == SupervisorState::Quarantined {
        broker.record_quarantine_trip();
    }

    *revision = revision.saturating_add(1);
    let previous = tx.borrow().clone();
    let mut current = (*previous).clone();
    current.version = DomainVersion::new(owner_generation, *revision);
    current.connection = target_lifecycle;
    current.capabilities = capabilities.capabilities_for(target_lifecycle);
    current.last_error = Some(error_msg);
    let snap_arc = Arc::new(current);
    if broker.observe_snapshot(snap_arc.clone()).is_ok() {
        let _ = tx.send(snap_arc);
    }
}

/// Applies a tick to the domain supervisor with the given timestamp.
pub fn apply_tick(supervisor: &Arc<Mutex<DomainSupervisor>>, now_ms: u64) {
    supervisor.lock().unwrap().tick(now_ms);
}

/// Sleeps for `duration` in short steps while periodically checking `stop_flag`.
pub fn sleep_with_stop_flag(duration: Duration, stop_flag: &AtomicBool) {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Shared supervisor state and channels for a compositor backend.
pub struct CompositorSupervision<C: CapabilityProvider> {
    pub supervisor: Arc<Mutex<DomainSupervisor>>,
    pub tx: watch::Sender<Arc<CompositorSnapshot>>,
    pub rx: watch::Receiver<Arc<CompositorSnapshot>>,
    pub broker: Arc<CompositorCommandBroker>,
    pub capability_provider: C,
}

impl<C: CapabilityProvider> CompositorSupervision<C> {
    pub fn new(
        initial_snapshot: CompositorSnapshot,
        broker: Arc<CompositorCommandBroker>,
        capability_provider: C,
    ) -> Self {
        let snap_arc = Arc::new(initial_snapshot);
        let (tx, rx) = watch::channel(snap_arc.clone());
        let supervisor = Arc::new(Mutex::new(DomainSupervisor::new()));

        if snap_arc.version.owner_generation > 0 {
            broker.set_installed_generation(snap_arc.version.owner_generation);
        }
        let _ = broker.observe_snapshot(snap_arc);

        Self {
            supervisor,
            tx,
            rx,
            broker,
            capability_provider,
        }
    }

    pub fn supervisor_state(&self) -> SupervisorState {
        self.supervisor.lock().unwrap().state()
    }

    pub fn begin_start(&self) {
        self.supervisor.lock().unwrap().mark_starting();

        let previous = self.rx.borrow().clone();
        let mut current = (*previous).clone();
        if current.version != DomainVersion::ZERO {
            let next_rev = current.version.revision.saturating_add(1);
            current.version = DomainVersion::new(current.version.owner_generation, next_rev);
        }
        current.connection = DomainLifecycle::Connecting;
        current.capabilities = self
            .capability_provider
            .capabilities_for(DomainLifecycle::Connecting);
        current.last_error = None;
        let snap_arc = Arc::new(current);
        if self.broker.observe_snapshot(snap_arc.clone()).is_ok() {
            let _ = self.tx.send(snap_arc);
        }

        tracing::info!(target: "shilpo_profile", lifecycle = "starting", "compositor supervisor transition");
    }

    pub fn mark_ready(&self, now_ms: u64) {
        self.supervisor.lock().unwrap().mark_running(now_ms);
        tracing::info!(target: "shilpo_profile", lifecycle = "ready", "compositor supervisor transition");
    }

    pub fn report_owner_failure(&self, error: String, now_ms: u64) {
        let version = self.rx.borrow().version;
        let owner_generation = version.owner_generation;
        let mut revision = version.revision;
        record_supervisor_failure(
            &self.supervisor,
            &self.broker,
            &self.tx,
            owner_generation,
            &mut revision,
            error,
            now_ms,
            &self.capability_provider,
        );
    }

    pub fn tick(&self, now_ms: u64) {
        apply_tick(&self.supervisor, now_ms);
    }

    pub fn update_snapshot(&self, snapshot: CompositorSnapshot) -> Result<(), StaleUpdateError> {
        let snap_arc = Arc::new(snapshot);
        self.broker.observe_snapshot(snap_arc.clone())?;
        let _ = self.tx.send(snap_arc);
        Ok(())
    }

    pub fn set_reconnecting_generation(&self, generation: u64) {
        let previous = self.rx.borrow().clone();
        let mut current = (*previous).clone();
        current.version = DomainVersion::new(generation, 0);
        current.connection = DomainLifecycle::Reconnecting;
        current.capabilities = self
            .capability_provider
            .capabilities_for(DomainLifecycle::Reconnecting);
        let snap_arc = Arc::new(current);
        let _ = self.tx.send(snap_arc);
    }

    pub fn reset_quarantine(&self) {
        if !self.supervisor.lock().unwrap().reset_quarantine() {
            return;
        }

        let previous = self.rx.borrow().clone();
        let mut current = (*previous).clone();
        let next_gen = current.version.owner_generation.saturating_add(1);
        current.version = DomainVersion::new(next_gen, 0);
        current.connection = DomainLifecycle::Reconnecting;
        current.capabilities = self
            .capability_provider
            .capabilities_for(DomainLifecycle::Reconnecting);
        let snap_arc = Arc::new(current);
        self.broker.set_installed_generation(next_gen);
        if self.broker.observe_snapshot(snap_arc.clone()).is_ok() {
            let _ = self.tx.send(snap_arc);
        }
    }
}
