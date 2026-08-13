use super::{
    DebugDbusService, DebugProxy, ShellCommand, ShellDbusService, ShellProxy, ShellStatus,
    ShellTelemetry,
};
use shilpo_observability::LogFilterController;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

fn create_mock_controller(initial: &str) -> LogFilterController {
    LogFilterController::new_for_testing(initial)
}

fn assert_method_error(error: zbus::Error, expected_name: &str) {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            assert_eq!(name.to_string(), expected_name);
        }
        other => panic!("expected D-Bus method error {expected_name}, got {other:?}"),
    }
}

async fn test_pair(
    service: ShellDbusService,
    debug_service: DebugDbusService,
    receiver: mpsc::Receiver<ShellCommand>,
) -> (
    zbus::Connection,
    zbus::Connection,
    mpsc::Receiver<ShellCommand>,
) {
    let (server_stream, client_stream) = UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let server_builder = zbus::connection::Builder::unix_stream(server_stream)
        .server(guid)
        .unwrap()
        .p2p()
        .serve_at("/org/shilpo/Shell", service)
        .unwrap()
        .serve_at("/org/shilpo/Shell", debug_service)
        .unwrap();
    let client_builder = zbus::connection::Builder::unix_stream(client_stream).p2p();
    let (server, client) =
        tokio::try_join!(server_builder.build(), client_builder.build()).unwrap();
    (server, client, receiver)
}

fn services_pair(
    controller: Option<LogFilterController>,
) -> (
    ShellDbusService,
    DebugDbusService,
    mpsc::Receiver<ShellCommand>,
) {
    let (tx, rx) = mpsc::channel(128);
    let shell_service = ShellDbusService::new(
        tx.clone(),
        Arc::new(Mutex::new(None)),
        Arc::new(arc_swap::ArcSwap::from_pointee(ShellStatus::default())),
        Arc::new(arc_swap::ArcSwap::from_pointee(ShellTelemetry::default())),
    );
    let debug_service = DebugDbusService::new(controller, tx);
    (shell_service, debug_service, rx)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn p2p_introspection_and_status_contract() {
    let controller = create_mock_controller("warn,shilpo=info");
    let (shell_service, debug_service, receiver) = services_pair(Some(controller));
    let (_server, client, _receiver) = test_pair(shell_service, debug_service, receiver).await;

    let shell_proxy = ShellProxy::builder(&client)
        .destination("org.shilpo.Shell")
        .unwrap()
        .build()
        .await
        .unwrap();
    let debug_proxy = DebugProxy::builder(&client)
        .destination("org.shilpo.Shell")
        .unwrap()
        .build()
        .await
        .unwrap();

    let status = shell_proxy.get_status().await.unwrap();
    assert_eq!(status, ShellStatus::default());

    let initial_filter = debug_proxy.get_log_filter().await.unwrap();
    assert_eq!(initial_filter, "warn,shilpo=info");

    let introspect = zbus::fdo::IntrospectableProxy::builder(&client)
        .destination("org.shilpo.Shell")
        .unwrap()
        .path("/org/shilpo/Shell")
        .unwrap()
        .build()
        .await
        .unwrap();

    let xml = tokio::time::timeout(std::time::Duration::from_secs(5), introspect.introspect())
        .await
        .expect("introspection response timed out")
        .unwrap();

    for member in [
        "org.shilpo.Shell",
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
        "ShellStarted",
        "ShellStopping",
        "WorkspaceChanged",
        "ThemeChanged",
        "ConfigReloaded",
        "org.shilpo.Debug",
        "SetLogFilter",
        "GetLogFilter",
        "EmitTestNotification",
    ] {
        assert!(xml.contains(member), "missing D-Bus member {member}");
    }

    drop(_server);
    p2p_debug_filter_and_notification_contract().await;
    p2p_closed_mailbox_and_unavailable_controller().await;
}

async fn p2p_debug_filter_and_notification_contract() {
    let controller = create_mock_controller("info");
    let (shell_service, debug_service, receiver) = services_pair(Some(controller));
    let (server, client, mut rx) = test_pair(shell_service, debug_service, receiver).await;

    let shell_proxy = ShellProxy::builder(&client)
        .destination("org.shilpo.Shell")
        .unwrap()
        .build()
        .await
        .unwrap();
    let debug_proxy = DebugProxy::builder(&client)
        .destination("org.shilpo.Shell")
        .unwrap()
        .build()
        .await
        .unwrap();

    // Valid filter change
    debug_proxy
        .set_log_filter("debug,shilpo_services=trace".into())
        .await
        .unwrap();
    assert_eq!(
        debug_proxy.get_log_filter().await.unwrap(),
        "debug,shilpo_services=trace"
    );

    // Invalid / empty filter preserves previous value and returns InvalidArgs
    let empty_err = debug_proxy.set_log_filter("   ".into()).await.unwrap_err();
    assert_method_error(empty_err, "org.freedesktop.DBus.Error.InvalidArgs");
    assert_eq!(
        debug_proxy.get_log_filter().await.unwrap(),
        "debug,shilpo_services=trace"
    );

    let malformed_err = debug_proxy
        .set_log_filter("invalid[[[syntax".into())
        .await
        .unwrap_err();
    assert_method_error(malformed_err, "org.freedesktop.DBus.Error.InvalidArgs");
    assert_eq!(
        debug_proxy.get_log_filter().await.unwrap(),
        "debug,shilpo_services=trace"
    );

    // Valid notification input yields exactly one EmitTestNotification command
    debug_proxy
        .emit_test_notification("Test Title".into(), "Test Body".into())
        .await
        .unwrap();

    let cmd = rx.recv().await.unwrap();
    assert_eq!(
        cmd,
        ShellCommand::EmitTestNotification {
            title: "Test Title".into(),
            body: "Test Body".into(),
        }
    );

    // Invalid title (empty) rejected without enqueueing
    let empty_title_err = debug_proxy
        .emit_test_notification("".into(), "Body".into())
        .await
        .unwrap_err();
    assert_method_error(empty_title_err, "org.freedesktop.DBus.Error.InvalidArgs");

    // Oversize title (>256 bytes) rejected
    let oversize_title = "a".repeat(257);
    let title_err = debug_proxy
        .emit_test_notification(oversize_title, "Body".into())
        .await
        .unwrap_err();
    assert_method_error(title_err, "org.freedesktop.DBus.Error.InvalidArgs");

    // Oversize body (>4096 bytes) rejected
    let oversize_body = "b".repeat(4097);
    let body_err = debug_proxy
        .emit_test_notification("Title".into(), oversize_body)
        .await
        .unwrap_err();
    assert_method_error(body_err, "org.freedesktop.DBus.Error.InvalidArgs");

    // Ensure no commands were enqueued for invalid calls
    assert!(rx.try_recv().is_err());

    // Argument validation on org.shilpo.Shell
    assert!(shell_proxy.set_brightness(101).await.is_err());

    // Mailbox overflow returns LimitsExceeded
    for _ in 0..128 {
        shell_proxy.show_bar().await.unwrap();
    }
    let overflow_err = debug_proxy
        .emit_test_notification("Overflow".into(), "Mailbox".into())
        .await
        .unwrap_err();
    assert_method_error(overflow_err, "org.freedesktop.DBus.Error.LimitsExceeded");

    drop(server);
}

async fn p2p_closed_mailbox_and_unavailable_controller() {
    let (tx, rx) = mpsc::channel(128);
    let shell_service = ShellDbusService::new(
        tx.clone(),
        Arc::new(Mutex::new(None)),
        Arc::new(arc_swap::ArcSwap::from_pointee(ShellStatus::default())),
        Arc::new(arc_swap::ArcSwap::from_pointee(ShellTelemetry::default())),
    );
    let debug_service = DebugDbusService::new(None, tx);
    drop(rx); // Close mailbox

    let (server_stream, client_stream) = UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let server_builder = zbus::connection::Builder::unix_stream(server_stream)
        .server(guid)
        .unwrap()
        .p2p()
        .serve_at("/org/shilpo/Shell", shell_service)
        .unwrap()
        .serve_at("/org/shilpo/Shell", debug_service)
        .unwrap();
    let client_builder = zbus::connection::Builder::unix_stream(client_stream).p2p();
    let (server, client) =
        tokio::try_join!(server_builder.build(), client_builder.build()).unwrap();

    let debug_proxy = DebugProxy::builder(&client)
        .destination("org.shilpo.Shell")
        .unwrap()
        .build()
        .await
        .unwrap();

    // Unavailable filter controller returns Failed error
    let get_err = debug_proxy.get_log_filter().await.unwrap_err();
    assert_method_error(get_err, "org.freedesktop.DBus.Error.Failed");

    let set_err = debug_proxy.set_log_filter("info".into()).await.unwrap_err();
    assert_method_error(set_err, "org.freedesktop.DBus.Error.Failed");

    // Closed mailbox returns Failed error
    let notif_err = debug_proxy
        .emit_test_notification("Closed".into(), "Mailbox".into())
        .await
        .unwrap_err();
    assert_method_error(notif_err, "org.freedesktop.DBus.Error.Failed");

    drop(server);
}
