use anyhow::Result;
use shilpo_services::{DeviceDaemonService, DeviceDbusService, SystemDeviceAdapter};
use std::sync::Arc;
use zbus::connection::Builder;
use zbus::object_server::SignalEmitter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("shilpo_services=info".parse().unwrap()),
        )
        .init();

    let adapter = Arc::new(SystemDeviceAdapter::new());
    let daemon = Arc::new(DeviceDaemonService::new(adapter));
    let mut outcomes = daemon.subscribe_outcomes();
    let service = DeviceDbusService::new(daemon);
    let connection = Builder::session()?
        .name("org.shilpo.Device")?
        .serve_at("/org/shilpo/Device", service.clone())?
        .build()
        .await?;
    let emitter = SignalEmitter::new(&connection, "/org/shilpo/Device")?.into_owned();

    tracing::info!("shilpo-device-daemon registered org.shilpo.Device");
    loop {
        tokio::select! {
            outcome = outcomes.recv() => {
                if let Ok(outcome) = outcome {
                    service.emit_outcome(&emitter, &outcome).await?;
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                service.emit_updates(&emitter).await?;
            }
        }
    }
}
