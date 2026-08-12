use super::{ShellDbusService, ShellProxy, ShellStatus, ShellTelemetry};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

async fn test_pair(
    service: ShellDbusService,
    receiver: mpsc::Receiver<super::ShellCommand>,
) -> (
    zbus::Connection,
    zbus::Connection,
    mpsc::Receiver<super::ShellCommand>,
) {
    let (server_stream, client_stream) = UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let server_builder = zbus::connection::Builder::unix_stream(server_stream)
        .server(guid)
        .unwrap()
        .p2p();
    let client_builder = zbus::connection::Builder::unix_stream(client_stream).p2p();
    let (server, client) =
        tokio::try_join!(server_builder.build(), client_builder.build()).unwrap();
    server
        .object_server()
        .at("/org/shilpo/Shell", service)
        .await
        .unwrap();
    (server, client, receiver)
}

fn service() -> (ShellDbusService, mpsc::Receiver<super::ShellCommand>) {
    let (tx, rx) = mpsc::channel(128);
    (
        ShellDbusService::new(
            tx,
            Arc::new(Mutex::new(None)),
            Arc::new(arc_swap::ArcSwap::from_pointee(ShellStatus::default())),
            Arc::new(arc_swap::ArcSwap::from_pointee(ShellTelemetry::default())),
        ),
        rx,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn p2p_introspection_and_status_contract() {
    let (service, receiver) = service();
    let (_server, client, _receiver) = test_pair(service, receiver).await;
    let proxy = ShellProxy::builder(&client)
        .destination("org.shilpo.Shell")
        .unwrap()
        .build()
        .await
        .unwrap();
    let status = proxy.get_status().await.unwrap();
    assert_eq!(status, ShellStatus::default());
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
    ] {
        assert!(xml.contains(member), "missing D-Bus member {member}");
    }

    drop(_server);
    p2p_argument_validation_and_mailbox_overflow().await;
}

async fn p2p_argument_validation_and_mailbox_overflow() {
    let (service, receiver) = service();
    let (server, client, _receiver) = test_pair(service, receiver).await;
    let proxy = ShellProxy::builder(&client)
        .destination("org.shilpo.Shell")
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(proxy.set_brightness(101).await.is_err());
    assert!(
        proxy
            .set_display_brightness(String::new(), 20)
            .await
            .is_err()
    );
    assert!(proxy.capture("unknown".into()).await.is_err());
    for _ in 0..128 {
        proxy.show_bar().await.unwrap();
    }
    assert!(proxy.show_bar().await.is_err());
    drop(server);
}
