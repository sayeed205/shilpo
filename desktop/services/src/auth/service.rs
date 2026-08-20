use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use super::helper::{AuthHelper, SystemAuthHelper};
use super::state::AuthDomainState;
use super::types::{
    AuthCommand, AuthCommandOutcome, AuthPort, AuthSnapshot, CommandTicket, DomainPortTelemetry,
    SupervisorState,
};

/// Service wrapper managing the lifecycle and event loop for the PAM authentication domain.
pub struct AuthService {
    adapter: Arc<AuthDomainState>,
}

impl Default for AuthService {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthService {
    /// Creates and starts a production `AuthService` using the real PAM helper.
    pub fn new() -> Self {
        let helper: Arc<dyn AuthHelper> = Arc::new(SystemAuthHelper::new());
        Self::with_helper(helper)
    }

    /// Creates an offline `AuthService` for tests without a real PAM subprocess.
    pub fn with_helper(helper: Arc<dyn AuthHelper>) -> Self {
        let adapter = Arc::new(AuthDomainState::new(32, helper));
        // Unlike the other revisioned domains (compositor, idle, ...), auth has no
        // persistent owner connection to wait for -- the PAM helper is spawned fresh per
        // authentication attempt, not held open by the supervisor. So the domain can (and
        // must) become ready synchronously here: `tokio::spawn` only schedules the
        // supervisor task, it does not run it, and callers like `run_lock()` call
        // `begin_authentication` immediately afterward with no `.await` in between, so a
        // deferred `mark_ready` would never win that race and every first command would be
        // rejected as `Unavailable`.
        adapter.begin_start();
        adapter.mark_ready(adapter.time_source().now_ms());
        Self::spawn_supervisor(adapter.clone());
        Self { adapter }
    }

    /// Creates a mock ready `AuthService` for test environments.
    pub fn new_mock() -> Self {
        let adapter = Arc::new(AuthDomainState::new_ready_for_test(
            32,
            Arc::new(super::helper::MockAuthHelper::new()),
        ));
        Self { adapter }
    }

    pub fn adapter(&self) -> Arc<AuthDomainState> {
        self.adapter.clone()
    }

    fn spawn_supervisor(adapter: Arc<AuthDomainState>) {
        tokio::spawn(async move {
            let time_source = adapter.time_source();
            let mut tick_interval = tokio::time::interval(Duration::from_millis(100));

            loop {
                tick_interval.tick().await;
                adapter.tick(time_source.now_ms());
                // Drain any helper events that arrived since the last tick. Bounded so a
                // pathological event storm can't starve the tick loop.
                for _ in 0..32 {
                    if adapter.poll_active_helper_event().is_none() {
                        break;
                    }
                }
            }
        });
    }
}

impl AuthPort for AuthService {
    fn snapshot(&self) -> AuthSnapshot {
        self.adapter.snapshot()
    }

    fn subscribe(&self) -> watch::Receiver<AuthSnapshot> {
        self.adapter.subscribe()
    }

    fn submit_command(&self, command: AuthCommand) -> Result<CommandTicket, AuthCommandOutcome> {
        // `AuthDomainState::submit_command` only enqueues (kept split from
        // `process_pending_commands` so the ADR-0006 conformance harness can drive the two
        // steps independently). This is the production entry point external callers use
        // (the lock screen UI), so it must drain the mailbox itself.
        let ticket = self.adapter.submit_command(command)?;
        self.adapter.process_pending_commands();
        Ok(ticket)
    }

    fn supervisor_state(&self) -> SupervisorState {
        self.adapter.supervisor_state()
    }

    fn telemetry(&self) -> DomainPortTelemetry {
        self.adapter.telemetry()
    }

    fn reset_quarantine(&self) {
        self.adapter.reset_quarantine();
    }
}
