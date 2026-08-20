use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, Mutex};

use zbus::Connection;

use super::types::IdleAction;
use crate::lock_supervisor::LockSupervisor;

/// Outcome of executing an idle action through an action sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionExecutionOutcome {
    /// Successfully executed by a registered handler.
    Executed,
    /// No handler registered for this action kind (unsupported).
    Unsupported,
    /// Handler exists but execution failed.
    Failed(String),
}

/// Interface for executing actions when an idle behavior fires.
pub trait IdleActionSink: Send + Sync {
    /// Returns true if a handler is registered for the specified action kind.
    fn has_handler_for(&self, action: &IdleAction) -> bool;

    /// Executes the specified action.
    fn execute_action(
        &self,
        behavior_name: &str,
        action: &IdleAction,
        lock_before_suspend: bool,
    ) -> ActionExecutionOutcome;
}

// ---------------------------------------------------------------------------
// Mock Idle Action Sink (for Tests)
// ---------------------------------------------------------------------------

/// In-memory mock action sink that records executed actions.
pub struct MockIdleActionSink {
    supported: Arc<Mutex<HashSet<&'static str>>>,
    executed: Arc<Mutex<Vec<(String, IdleAction, bool)>>>,
}

impl Default for MockIdleActionSink {
    fn default() -> Self {
        Self::new()
    }
}

impl MockIdleActionSink {
    pub fn new() -> Self {
        let mut supported = HashSet::new();
        supported.insert("none");
        supported.insert("suspend");
        supported.insert("lock_and_suspend");
        supported.insert("command");
        Self {
            supported: Arc::new(Mutex::new(supported)),
            executed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn set_supported(&self, action_name: &'static str, is_supported: bool) {
        let mut guard = self.supported.lock().unwrap();
        if is_supported {
            guard.insert(action_name);
        } else {
            guard.remove(action_name);
        }
    }

    pub fn executed_actions(&self) -> Vec<(String, IdleAction, bool)> {
        self.executed.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.executed.lock().unwrap().clear();
    }
}

impl IdleActionSink for MockIdleActionSink {
    fn has_handler_for(&self, action: &IdleAction) -> bool {
        self.supported.lock().unwrap().contains(action.name())
    }

    fn execute_action(
        &self,
        behavior_name: &str,
        action: &IdleAction,
        lock_before_suspend: bool,
    ) -> ActionExecutionOutcome {
        if !self.has_handler_for(action) {
            // If LockAndSuspend and lock handler missing, it still suspends if suspend supported
            if matches!(action, IdleAction::LockAndSuspend)
                && self.supported.lock().unwrap().contains("suspend")
            {
                self.executed.lock().unwrap().push((
                    behavior_name.to_string(),
                    action.clone(),
                    lock_before_suspend,
                ));
                return ActionExecutionOutcome::Executed;
            }
            return ActionExecutionOutcome::Unsupported;
        }

        self.executed.lock().unwrap().push((
            behavior_name.to_string(),
            action.clone(),
            lock_before_suspend,
        ));
        ActionExecutionOutcome::Executed
    }
}

// ---------------------------------------------------------------------------
// System Idle Action Sink
// ---------------------------------------------------------------------------

/// Production action sink that interfaces with systemd-logind via D-Bus and shells out custom commands.
pub struct SystemIdleActionSink {
    system_conn: Option<Connection>,
    lock_supervisor: Arc<LockSupervisor>,
}

impl SystemIdleActionSink {
    pub fn new(system_conn: Option<Connection>, lock_supervisor: Arc<LockSupervisor>) -> Self {
        Self {
            system_conn,
            lock_supervisor,
        }
    }
}

impl IdleActionSink for SystemIdleActionSink {
    fn has_handler_for(&self, action: &IdleAction) -> bool {
        match action {
            IdleAction::None
            | IdleAction::Lock
            | IdleAction::Suspend
            | IdleAction::LockAndSuspend
            | IdleAction::Command { .. } => true,
            IdleAction::ScreenOff | IdleAction::ScreenOn => false,
        }
    }

    fn execute_action(
        &self,
        behavior_name: &str,
        action: &IdleAction,
        lock_before_suspend: bool,
    ) -> ActionExecutionOutcome {
        match action {
            IdleAction::None => ActionExecutionOutcome::Executed,
            IdleAction::Lock => {
                self.lock_supervisor
                    .spawn(&format!("idle behavior '{behavior_name}'"));
                ActionExecutionOutcome::Executed
            }
            IdleAction::ScreenOff => ActionExecutionOutcome::Unsupported,
            IdleAction::ScreenOn => ActionExecutionOutcome::Unsupported,
            IdleAction::Command { command } => {
                let cmd_str = command.clone();
                // Spawn detached subprocess without blocking the owner
                let _ = Command::new("sh").args(["-c", &cmd_str]).spawn();
                ActionExecutionOutcome::Executed
            }
            IdleAction::Suspend | IdleAction::LockAndSuspend => {
                let should_lock =
                    lock_before_suspend || matches!(action, IdleAction::LockAndSuspend);
                if should_lock {
                    self.lock_supervisor.spawn(&format!(
                        "suspend triggered by idle behavior '{behavior_name}'"
                    ));
                }

                // Call systemd-logind Suspend(false)
                let conn = self.system_conn.clone();
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        if should_lock {
                            // Give the locker a moment to acquire the session lock and
                            // render before suspend freezes the process. This is a
                            // best-effort head start, not a guarantee: the authoritative
                            // "wait for locked before suspend" path is the
                            // PrepareForSleep + delay-inhibitor watch, which covers
                            // suspend triggered from any source, not just this action.
                            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        }

                        let system_conn = match conn {
                            Some(c) => c,
                            None => match Connection::system().await {
                                Ok(c) => c,
                                Err(e) => {
                                    tracing::error!(%e, "failed to connect to system bus for suspend");
                                    return;
                                }
                            },
                        };

                        let res = system_conn
                            .call_method(
                                Some("org.freedesktop.login1"),
                                "/org/freedesktop/login1",
                                Some("org.freedesktop.login1.Manager"),
                                "Suspend",
                                &(false,),
                            )
                            .await;

                        if let Err(err) = res {
                            tracing::error!(%err, "logind Suspend method call failed");
                        }
                    });
                }

                ActionExecutionOutcome::Executed
            }
        }
    }
}
