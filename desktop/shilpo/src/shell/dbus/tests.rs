use super::{ShellDbusService, ShellProxy, ShellStatus, ShellTelemetry};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

async fn test_pair(service: ShellDbusService) -> (zbus::Connection, zbus::Connection) {
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
    (server, client)
}

fn service() -> ShellDbusService {
    let (tx, rx) = mpsc::channel(128);
    // Keep the receiver alive so this exercises mailbox capacity rather than
    // the production stopping/closed-mailbox error path.
    std::mem::forget(rx);
    ShellDbusService::new(
        tx,
        Arc::new(Mutex::new(None)),
        Arc::new(arc_swap::ArcSwap::from_pointee(ShellStatus::default())),
        Arc::new(arc_swap::ArcSwap::from_pointee(ShellTelemetry::default())),
    )
}

#[tokio::test]
async fn p2p_introspection_and_status_contract() {
    let (_server, client) = test_pair(service()).await;
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
    let xml = introspect.introspect().await.unwrap();
    assert!(xml.contains("org.shilpo.Shell"));
    assert!(xml.contains("ConfigReloaded"));
    assert!(xml.contains("SetDisplayBrightness"));
}

#[tokio::test]
async fn p2p_argument_validation_and_mailbox_overflow() {
    let (server, client) = test_pair(service()).await;
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
