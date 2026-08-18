use std::sync::Arc;
use tokio::sync::watch;

use super::{
    BrokerOptions, CompositorAdapter, CompositorCapabilities, CompositorCommandBroker,
    CompositorExtras, CompositorSnapshot, DomainLifecycle, DomainVersion, RejectionReason,
    WindowIdentity,
};

/// Tier 0 Null compositor backend fallback.
///
/// Used when no compositor is recognized or no candidate backend can connect.
/// Reports `DomainLifecycle::Unavailable`, `WindowIdentity::None`, and all capabilities `false`.
pub struct NullCompositorBackend {
    snapshot: Arc<CompositorSnapshot>,
    tx: watch::Sender<Arc<CompositorSnapshot>>,
    broker: Arc<CompositorCommandBroker>,
}

impl NullCompositorBackend {
    pub fn new() -> Arc<Self> {
        let snapshot = Arc::new(CompositorSnapshot {
            version: DomainVersion::ZERO,
            connection: DomainLifecycle::Unavailable,
            capabilities: CompositorCapabilities {
                window_identity: WindowIdentity::None,
                can_create_workspace: false,
                can_move_window: false,
                can_focus_window: false,
                can_focus_workspace: false,
                can_close_window: false,
            },
            outputs: Vec::new(),
            workspaces: Vec::new(),
            windows: Vec::new(),
            focused_output: None,
            focused_workspace_id: None,
            focused_window_id: None,
            active_keyboard_layout: None,
            extras: CompositorExtras::None,
            last_error: None,
        });

        let (tx, _rx) = watch::channel(snapshot.clone());
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Err(RejectionReason::Unsupported)),
        );
        let _ = broker.observe_snapshot(snapshot.clone());

        Arc::new(Self {
            snapshot,
            tx,
            broker,
        })
    }
}

impl Default for NullCompositorBackend {
    fn default() -> Self {
        let snapshot = Arc::new(CompositorSnapshot::default());
        let (tx, _rx) = watch::channel(snapshot.clone());
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Err(RejectionReason::Unsupported)),
        );
        let _ = broker.observe_snapshot(snapshot.clone());

        Self {
            snapshot,
            tx,
            broker,
        }
    }
}

impl CompositorAdapter for NullCompositorBackend {
    fn current(&self) -> Arc<CompositorSnapshot> {
        self.snapshot.clone()
    }

    fn subscribe(&self) -> watch::Receiver<Arc<CompositorSnapshot>> {
        self.tx.subscribe()
    }

    fn command_broker(&self) -> Arc<CompositorCommandBroker> {
        self.broker.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandOutcome;
    use crate::compositor::CompositorCommand;

    #[test]
    fn test_null_backend_reports_unavailable_and_all_caps_false() {
        let backend = NullCompositorBackend::new();
        let snapshot = backend.current();

        assert_eq!(snapshot.connection, DomainLifecycle::Unavailable);
        assert_eq!(snapshot.capabilities.window_identity, WindowIdentity::None);
        assert!(!snapshot.capabilities.can_create_workspace);
        assert!(!snapshot.capabilities.can_move_window);
        assert!(!snapshot.capabilities.can_focus_window);
        assert!(!snapshot.capabilities.can_focus_workspace);
        assert!(!snapshot.capabilities.can_close_window);
    }

    #[test]
    fn test_null_backend_rejects_every_command_as_unsupported() {
        let backend = NullCompositorBackend::new();
        let broker = backend.command_broker();

        let commands = [
            CompositorCommand::CreateWorkspace,
            CompositorCommand::FocusWorkspace(1),
            CompositorCommand::FocusWindow(10),
            CompositorCommand::FocusPreviousWindow,
            CompositorCommand::CloseWindow(10),
            CompositorCommand::MoveWindowToWorkspace {
                window_id: 10,
                workspace_id: 1,
            },
        ];

        for cmd in commands {
            let res = broker.submit(cmd);
            assert!(matches!(
                res,
                Err(CommandOutcome::Rejected {
                    reason: RejectionReason::Unavailable | RejectionReason::Unsupported
                })
            ));
        }
    }

    #[test]
    fn test_null_backend_subscribe_does_not_panic() {
        let backend = NullCompositorBackend::new();
        let mut rx = backend.subscribe();
        let initial = rx.borrow_and_update().clone();
        assert_eq!(initial.connection, DomainLifecycle::Unavailable);
    }
}
