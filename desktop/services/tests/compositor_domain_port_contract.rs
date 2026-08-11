use shilpo_services::{
    BrokerOptions, CancellationReason, CommandExecutorFn, CommandOutcome, CompositorAdapter,
    CompositorCommand, CompositorCommandBroker, CompositorConnection, CompositorSnapshot,
    DomainVersion, ExecutorAck, MailboxPolicy, StaleUpdateError, TestCompositorAdapter, WindowInfo,
    WorkspaceInfo,
};
use std::sync::Arc;
use std::time::Duration;

fn ready_snapshot(generation: u64, rev: u64) -> CompositorSnapshot {
    CompositorSnapshot {
        version: DomainVersion::new(generation, rev),
        connection: CompositorConnection::Ready,
        workspaces: vec![WorkspaceInfo {
            id: 1,
            name: None,
            idx: 1,
            is_active: true,
            is_focused: true,
            is_urgent: false,
            output_name: None,
            active_window_id: None,
        }],
        windows: vec![WindowInfo {
            id: 10,
            title: None,
            app_id: None,
            workspace_id: Some(1),
            is_focused: true,
            is_floating: false,
            is_urgent: false,
            layout_x: None,
            layout_y: None,
            column: None,
            row: None,
        }],
        focused_workspace_id: Some(1),
        focused_window_id: Some(10),
        ..Default::default()
    }
}

#[test]
fn test_compositor_version_ordering_and_zero() {
    assert!(DomainVersion::ZERO < DomainVersion::new(1, 0));
    assert!(DomainVersion::new(1, 100) < DomainVersion::new(2, 0));
    assert_eq!(DomainVersion::new(1, 5).to_string(), "g1.r5");
}

#[test]
fn test_compositor_snapshot_stale_and_conflicting_updates() {
    let adapter = TestCompositorAdapter::new(ready_snapshot(1, 10));
    let snap = ready_snapshot(1, 5);
    let err = adapter.update_result(snap).unwrap_err();
    assert!(matches!(err, StaleUpdateError::StaleVersion { .. }));

    let mut conflicting = ready_snapshot(1, 10);
    conflicting.focused_workspace_id = Some(2);
    let err_conflict = adapter.update_result(conflicting).unwrap_err();
    assert!(matches!(
        err_conflict,
        StaleUpdateError::ConflictingSnapshot { .. }
    ));

    let uninstalled = ready_snapshot(5, 0);
    let err_uninst = adapter.update_result(uninstalled).unwrap_err();
    assert!(matches!(
        err_uninst,
        StaleUpdateError::UninstalledGeneration { .. }
    ));
}

#[tokio::test]
async fn test_compositor_generation_fence_cancels_previous_commands() {
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let release = Arc::new(std::sync::Barrier::new(2));
    let release_executor = release.clone();
    let executor: CommandExecutorFn = Box::new(move |_cmd, _timeout, _cancel, _register| {
        started_tx.send(()).unwrap();
        release_executor.wait();
        Ok(ExecutorAck::Success)
    });
    let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
    broker.set_installed_generation(1);
    let _ = broker.observe_snapshot(Arc::new(ready_snapshot(1, 1)));

    let t1 = broker.submit(CompositorCommand::FocusWorkspace(1)).unwrap();
    started_rx.recv().unwrap();
    let t2 = broker.submit(CompositorCommand::FocusWindow(10)).unwrap();

    // Owner generation replaced
    broker.set_installed_generation(2);

    release.wait();
    assert_eq!(
        t1.await,
        CommandOutcome::Cancelled {
            reason: CancellationReason::OwnerReplaced
        }
    );
    assert_eq!(
        t2.await,
        CommandOutcome::Cancelled {
            reason: CancellationReason::OwnerReplaced
        }
    );
}

#[tokio::test]
async fn test_compositor_mailbox_policy_supersedes_duplicate_key() {
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let release = Arc::new(std::sync::Barrier::new(2));
    let release_executor = release.clone();
    let first = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let first_executor = first.clone();
    let executor: CommandExecutorFn = Box::new(move |_cmd, _timeout, _cancel, _register| {
        if first_executor.swap(false, std::sync::atomic::Ordering::AcqRel) {
            started_tx.send(()).unwrap();
            release_executor.wait();
        }
        Ok(ExecutorAck::Success)
    });
    let broker = CompositorCommandBroker::new(
        BrokerOptions {
            timeout: Duration::from_secs(1),
            max_queue_len: 10,
        },
        executor,
    );
    broker.set_installed_generation(1);
    let _ = broker.observe_snapshot(Arc::new(ready_snapshot(1, 1)));

    let blocker = broker.submit(CompositorCommand::CloseWindow(10)).unwrap();
    started_rx.recv().unwrap();

    let t1 = broker
        .submit_with_policy(
            CompositorCommand::FocusWorkspace(1),
            MailboxPolicy::ReplaceLatest {
                key: "workspace_nav".into(),
            },
        )
        .unwrap();

    let t2 = broker
        .submit_with_policy(
            CompositorCommand::FocusWorkspace(1),
            MailboxPolicy::ReplaceLatest {
                key: "workspace_nav".into(),
            },
        )
        .unwrap();

    assert_eq!(
        t1.await,
        CommandOutcome::Cancelled {
            reason: CancellationReason::Superseded
        }
    );

    let telem = broker.telemetry();
    assert_eq!(telem.supersessions, 1);
    release.wait();
    let _ = blocker.await;
    let _ = t2.await;
}

#[test]
fn test_compositor_telemetry_fields() {
    let adapter = TestCompositorAdapter::new(ready_snapshot(1, 1));
    adapter.set_installed_generation(1);
    let telem = adapter.command_broker().telemetry();
    assert_eq!(telem.owner_generation, 1);
    assert_eq!(telem.queue_capacity, 32);
    assert_eq!(telem.current_queue_depth, 0);
    assert_eq!(telem.stale_updates, 0);
}
