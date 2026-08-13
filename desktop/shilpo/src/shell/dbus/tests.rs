use super::{
    DebugDbusService, ShellCommand, ShellDbusService, ShellStatus, ShellTelemetry,
    test_harness::TestDbusHarness,
};
use futures_lite::stream::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

fn assert_method_error(error: zbus::Error, expected_name: &str) {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            assert_eq!(name.to_string(), expected_name);
        }
        other => panic!("expected D-Bus method error {expected_name}, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_introspection_exact_contract() {
    let harness = TestDbusHarness::new().await;
    let xml = harness.introspect_xml().await;

    assert!(xml.contains("interface name=\"org.shilpo.Shell\""));
    assert!(xml.contains("interface name=\"org.shilpo.Debug\""));

    // 18 org.shilpo.Shell methods
    for method in [
        "ReloadConfig",
        "ShowBar",
        "HideBar",
        "ToggleBar",
        "ShowOverview",
        "HideOverview",
        "ToggleOverview",
        "FocusWorkspace",
        "CreateWorkspace",
        "FocusWindow",
        "FocusPreviousWindow",
        "CloseWindow",
        "MoveWindowToWorkspace",
        "SetBrightness",
        "SetDisplayBrightness",
        "GetStatus",
        "GetTelemetry",
        "Capture",
    ] {
        assert!(
            xml.contains(&format!("method name=\"{method}\"")),
            "missing method {method} in introspection XML"
        );
    }

    // 5 org.shilpo.Shell signals
    for signal in [
        "ShellStarted",
        "ShellStopping",
        "WorkspaceChanged",
        "ThemeChanged",
        "ConfigReloaded",
    ] {
        assert!(
            xml.contains(&format!("signal name=\"{signal}\"")),
            "missing signal {signal} in introspection XML"
        );
    }

    // 3 org.shilpo.Debug methods
    for debug_method in ["SetLogFilter", "GetLogFilter", "EmitTestNotification"] {
        assert!(
            xml.contains(&format!("method name=\"{debug_method}\"")),
            "missing debug method {debug_method} in introspection XML"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_mailbox_command_delivery_and_fifo() {
    let mut harness = TestDbusHarness::new().await;

    harness.shell_proxy.reload_config().await.unwrap();
    harness.shell_proxy.show_bar().await.unwrap();
    harness.shell_proxy.set_brightness(42).await.unwrap();

    let cmd1 = harness.mailbox_rx.recv().await.unwrap();
    let cmd2 = harness.mailbox_rx.recv().await.unwrap();
    let cmd3 = harness.mailbox_rx.recv().await.unwrap();

    assert_eq!(cmd1, ShellCommand::ReloadConfig);
    assert_eq!(cmd2, ShellCommand::ShowBar);
    assert_eq!(cmd3, ShellCommand::SetBrightness(42));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_boundary_error_handling() {
    let mut harness = TestDbusHarness::new().await;

    // Invalid brightness (>100) -> InvalidArgs
    let err = harness.shell_proxy.set_brightness(101).await.unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.InvalidArgs");

    // Invalid display brightness (empty display_id or >100) -> InvalidArgs
    let err = harness
        .shell_proxy
        .set_display_brightness("".into(), 50)
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.InvalidArgs");

    let err = harness
        .shell_proxy
        .set_display_brightness("DP-1".into(), 101)
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.InvalidArgs");

    // Invalid capture intent -> InvalidArgs
    let err = harness
        .shell_proxy
        .capture("invalid_intent".into())
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.InvalidArgs");

    // Invalid debug inputs -> InvalidArgs
    let err = harness
        .debug_proxy
        .set_log_filter("   ".into())
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.InvalidArgs");

    let err = harness
        .debug_proxy
        .set_log_filter("invalid[[syntax".into())
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.InvalidArgs");

    let err = harness
        .debug_proxy
        .emit_test_notification("".into(), "body".into())
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.InvalidArgs");

    let err = harness
        .debug_proxy
        .emit_test_notification("a".repeat(257), "body".into())
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.InvalidArgs");

    let err = harness
        .debug_proxy
        .emit_test_notification("title".into(), "b".repeat(4097))
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.InvalidArgs");

    // Mailbox rejected calls enqueue nothing
    assert!(harness.mailbox_rx.try_recv().is_err());

    // Mailbox overflow -> LimitsExceeded
    for _ in 0..128 {
        harness.shell_proxy.show_bar().await.unwrap();
    }
    let overflow_err = harness
        .debug_proxy
        .emit_test_notification("Overflow".into(), "Mailbox".into())
        .await
        .unwrap_err();
    assert_method_error(overflow_err, "org.freedesktop.DBus.Error.LimitsExceeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_closed_mailbox_and_unavailable_controller() {
    let (tx, rx) = mpsc::channel(128);
    let shell_service = ShellDbusService::new(
        tx.clone(),
        std::sync::Arc::new(std::sync::Mutex::new(None)),
        std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(ShellStatus::default())),
        std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(ShellTelemetry::default())),
    );
    let debug_service = DebugDbusService::new(None, tx);
    drop(rx); // Close receiver

    let harness =
        TestDbusHarness::new_with_services(shell_service, debug_service, mpsc::channel(1).1).await;

    let err = harness.debug_proxy.get_log_filter().await.unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.Failed");

    let err = harness
        .debug_proxy
        .set_log_filter("info".into())
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.Failed");

    let err = harness
        .debug_proxy
        .emit_test_notification("Title".into(), "Body".into())
        .await
        .unwrap_err();
    assert_method_error(err, "org.freedesktop.DBus.Error.Failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_atomic_status_and_telemetry_snapshots() {
    let harness = TestDbusHarness::new().await;

    let custom_status = ShellStatus {
        running: true,
        instance_id: "test-instance-123".into(),
        pid: 4321,
        readiness: "ready".into(),
        bar_state: "visible".into(),
        overview_visible: true,
    };
    harness.update_status(custom_status.clone());

    let status = harness.shell_proxy.get_status().await.unwrap();
    assert_eq!(status, custom_status);

    let custom_telemetry = ShellTelemetry {
        compositor_connected: true,
        compositor_state: "connected".into(),
        compositor_owner_generation: 12,
        compositor_revision: 34,
        uptime_seconds: 12345,
        ..Default::default()
    };
    harness.update_telemetry(custom_telemetry.clone());

    let telemetry = harness.shell_proxy.get_telemetry().await.unwrap();
    assert_eq!(telemetry, custom_telemetry);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_lifecycle_signals() {
    let harness = TestDbusHarness::new().await;

    let mut started_stream = harness.shell_proxy.receive_shell_started().await.unwrap();
    let mut stopping_stream = harness.shell_proxy.receive_shell_stopping().await.unwrap();

    let emitter = harness.signal_emitter().await;
    ShellDbusService::shell_started(&emitter, "inst-42", 1234)
        .await
        .unwrap();
    ShellDbusService::shell_stopping(&emitter, "inst-42")
        .await
        .unwrap();

    let started_sig = tokio::time::timeout(Duration::from_secs(2), started_stream.next())
        .await
        .expect("started signal timeout")
        .expect("started stream item");
    let started_args = started_sig.args().unwrap();
    assert_eq!(started_args.instance_id, "inst-42");
    assert_eq!(started_args.pid, 1234);

    let stopping_sig = tokio::time::timeout(Duration::from_secs(2), stopping_stream.next())
        .await
        .expect("stopping signal timeout")
        .expect("stopping stream item");
    let stopping_args = stopping_sig.args().unwrap();
    assert_eq!(stopping_args.instance_id, "inst-42");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_workspace_changed_dedup_semantics() {
    let harness = TestDbusHarness::new().await;
    let mut stream = harness
        .shell_proxy
        .receive_workspace_changed()
        .await
        .unwrap();
    let emitter = harness.signal_emitter().await;

    // Prime workspace 1 (initial priming emits nothing)
    harness.shell_service.prime_workspace(1);

    // Repeated workspace 1 emits nothing
    harness
        .shell_service
        .emit_workspace_changed_if_needed(&emitter, 1, 10, 100)
        .await;

    // Change to workspace 2 emits once
    harness
        .shell_service
        .emit_workspace_changed_if_needed(&emitter, 2, 10, 101)
        .await;

    let sig = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("workspace changed timeout")
        .expect("workspace changed item");
    let args = sig.args().unwrap();
    assert_eq!(args.workspace_id, 2);
    assert_eq!(args.owner_generation, 10);
    assert_eq!(args.revision, 101);

    // Repeat workspace 2 with updated generation/revision emits nothing
    harness
        .shell_service
        .emit_workspace_changed_if_needed(&emitter, 2, 11, 102)
        .await;

    let no_sig = tokio::time::timeout(Duration::from_millis(100), stream.next()).await;
    assert!(no_sig.is_err(), "expected no signal for repeated workspace");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_theme_changed_dedup_semantics() {
    let harness = TestDbusHarness::new().await;
    let mut stream = harness.shell_proxy.receive_theme_changed().await.unwrap();
    let emitter = harness.signal_emitter().await;

    // First call populates initial state and emits nothing
    harness
        .shell_service
        .emit_theme_changed_if_needed(&emitter, "dark", "Expressive")
        .await;

    // Changed mode emits once
    harness
        .shell_service
        .emit_theme_changed_if_needed(&emitter, "light", "Expressive")
        .await;

    let sig = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("theme changed timeout")
        .expect("theme changed item");
    let args = sig.args().unwrap();
    assert_eq!(args.mode, "light");
    assert_eq!(args.scheme_variant, "Expressive");

    // Equal repeat emits nothing
    harness
        .shell_service
        .emit_theme_changed_if_needed(&emitter, "light", "Expressive")
        .await;

    let no_sig = tokio::time::timeout(Duration::from_millis(100), stream.next()).await;
    assert!(no_sig.is_err(), "expected no signal for repeated theme");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_config_reloaded_signal_semantics() {
    let harness = TestDbusHarness::new().await;
    let mut stream = harness.shell_proxy.receive_config_reloaded().await.unwrap();
    let emitter = harness.signal_emitter().await;

    // Successful reload sorts component names
    harness
        .shell_service
        .emit_config_reloaded(&emitter, true, vec!["theme".into(), "bar".into()], 0)
        .await;

    let sig1 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("config reloaded timeout")
        .expect("config reloaded item");
    let args1 = sig1.args().unwrap();
    assert!(args1.success);
    assert_eq!(args1.changed_components, vec!["bar", "theme"]);
    assert_eq!(args1.diagnostic_count, 0);

    // Failed reload clears component list but preserves diagnostic count
    harness
        .shell_service
        .emit_config_reloaded(&emitter, false, vec!["bar".into(), "theme".into()], 5)
        .await;

    let sig2 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("config reloaded timeout")
        .expect("config reloaded item");
    let args2 = sig2.args().unwrap();
    assert!(!args2.success);
    assert!(args2.changed_components.is_empty());
    assert_eq!(args2.diagnostic_count, 5);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_clean_lifecycle_drop() {
    let harness = TestDbusHarness::new().await;
    harness.shell_proxy.get_status().await.unwrap();
    drop(harness);
}
