//! Backend adapters for the power profile service.
//!
//! This module hides the system IPC (D-Bus) protocol behind a narrow seam so
//! the rest of the application never touches `zbus`. Each adapter owns an
//! async event loop that watches the daemon for `PropertiesChanged` signals
//! and forwards the updated state to the harness via a `watch::Sender`.

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::{PowerProfileCommand, PowerProfileInfo};

/// Seam between the harness and a concrete power profile backend.
pub(crate) trait PowerProfileAdapter: Send + 'static {
    /// Spawn the backend event loop. The loop owns `tx` and `command_rx` and
    /// runs until the harness is dropped.
    fn spawn(
        self,
        tx: watch::Sender<PowerProfileInfo>,
        command_rx: mpsc::UnboundedReceiver<PowerProfileCommand>,
    ) -> JoinHandle<()>;
}

#[cfg(target_os = "linux")]
pub(crate) use dbus::ZbusPowerProfileAdapter;

#[cfg(target_os = "linux")]
pub(crate) mod dbus {
    //! D-Bus backend backed by the `net.hadess.PowerProfiles` daemon.

    use std::time::Duration;

    use futures_lite::StreamExt;
    use tokio::sync::{mpsc, watch};
    use tokio::task::JoinHandle;

    use super::PowerProfileAdapter;
    use crate::power_profile::{PowerProfileCommand, PowerProfileInfo};

    /// The system bus name, object path, and interface of the daemon.
    const POWER_PROFILES_BUS: &str = "net.hadess.PowerProfiles";
    const POWER_PROFILES_PATH: &str = "/net/hadess/PowerProfiles";
    const POWER_PROFILES_IFACE: &str = "net.hadess.PowerProfiles";
    const RECONNECT_DELAY: Duration = Duration::from_secs(2);

    /// Proxy for the `net.hadess.PowerProfiles` interface.
    #[zbus::proxy(
        interface = "net.hadess.PowerProfiles",
        default_service = "net.hadess.PowerProfiles",
        default_path = "/net/hadess/PowerProfiles"
    )]
    trait PowerProfiles {
        #[zbus(property)]
        fn active_profile(&self) -> zbus::Result<String>;

        #[zbus(property)]
        fn set_active_profile(&self, value: &str) -> zbus::Result<()>;
    }

    /// Property names exposed by the daemon.
    const ACTIVE_PROFILE_PROP: &str = "ActiveProfile";
    const PROFILES_PROP: &str = "Profiles";

    /// D-Bus backed adapter for the power profiles daemon.
    pub(crate) struct ZbusPowerProfileAdapter;

    impl PowerProfileAdapter for ZbusPowerProfileAdapter {
        fn spawn(
            self,
            tx: watch::Sender<PowerProfileInfo>,
            command_rx: mpsc::UnboundedReceiver<PowerProfileCommand>,
        ) -> JoinHandle<()> {
            tokio::spawn(run_loop(tx, command_rx))
        }
    }

    async fn setup_connection_and_proxies() -> zbus::Result<(
        zbus::Connection,
        PowerProfilesProxy<'static>,
        zbus::fdo::PropertiesProxy<'static>,
        zbus::fdo::PropertiesChangedStream,
    )> {
        let connection = zbus::Connection::system().await?;
        let proxy = PowerProfilesProxy::new(&connection).await?;
        let properties = properties_proxy(&connection).await?;
        let changes = properties.receive_properties_changed().await?;
        Ok((connection, proxy, properties, changes))
    }

    async fn run_loop(
        tx: watch::Sender<PowerProfileInfo>,
        mut command_rx: mpsc::UnboundedReceiver<PowerProfileCommand>,
    ) {
        loop {
            let (_connection, proxy, _properties, mut changes) =
                match setup_connection_and_proxies().await {
                    Ok(tuple) => tuple,
                    Err(error) => {
                        retry_after_disconnect(
                            &tx,
                            format!("failed to initialize power profile D-Bus connection: {error}"),
                        )
                        .await;
                        continue;
                    }
                };

            if let Ok(active) = proxy.active_profile().await {
                let _ = tx.send_replace(PowerProfileInfo::online(&active));
            } else {
                let _ = tx.send_replace(PowerProfileInfo::offline());
            }

            loop {
                tokio::select! {
                    command = command_rx.recv() => {
                        match command {
                            Some(PowerProfileCommand::Set(profile)) => {
                                if proxy.set_active_profile(profile.as_str()).await.is_ok() {
                                    let _ = tx.send_replace(PowerProfileInfo::live(profile));
                                } else if let Ok(active) = proxy.active_profile().await {
                                    let _ = tx.send_replace(PowerProfileInfo::online(&active));
                                }
                            }
                            None => return,
                        }
                    }
                    change = changes.next() => {
                        let Some(change) = change else {
                            break;
                        };
                        let Ok(args) = change.args() else {
                            continue;
                        };
                        if args.interface_name.as_str() == POWER_PROFILES_IFACE
                            && (args.changed_properties.contains_key(ACTIVE_PROFILE_PROP)
                                || args.changed_properties.contains_key(PROFILES_PROP)
                                || args.invalidated_properties.contains(&ACTIVE_PROFILE_PROP)
                                || args.invalidated_properties.contains(&PROFILES_PROP))
                            && let Ok(active) = proxy.active_profile().await
                        {
                            let _ = tx.send_replace(PowerProfileInfo::online(&active));
                        }
                    }
                }
            }

            tracing::debug!("power profile: system bus connection lost; reconnecting");
            let _ = tx.send_replace(PowerProfileInfo::offline());
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    }

    async fn retry_after_disconnect(tx: &watch::Sender<PowerProfileInfo>, reason: String) {
        tracing::warn!("power profile: {reason}; reconnecting");
        let _ = tx.send_replace(PowerProfileInfo::offline());
        tokio::time::sleep(RECONNECT_DELAY).await;
    }

    async fn properties_proxy(
        connection: &zbus::Connection,
    ) -> zbus::Result<zbus::fdo::PropertiesProxy<'static>> {
        zbus::fdo::PropertiesProxy::builder(connection)
            .destination(POWER_PROFILES_BUS)?
            .path(POWER_PROFILES_PATH)?
            .build()
            .await
    }
}

#[cfg(test)]
pub(crate) mod mock {
    //! In-memory backend that simulates `PropertiesChanged` events for tests.

    use tokio::sync::{mpsc, watch};
    use tokio::task::JoinHandle;

    use super::PowerProfileAdapter;
    use crate::power_profile::{PowerProfileCommand, PowerProfileInfo};

    /// Test backend whose event source is driven by the test.
    pub(crate) struct TestPowerProfileAdapter {
        event_rx: mpsc::UnboundedReceiver<PowerProfileInfo>,
    }

    impl TestPowerProfileAdapter {
        /// Creates a new test adapter together with a handle used to emit
        /// simulated `PropertiesChanged` events.
        pub(crate) fn new() -> (Self, mpsc::UnboundedSender<PowerProfileInfo>) {
            let (event_tx, event_rx) = mpsc::unbounded_channel();
            (Self { event_rx }, event_tx)
        }
    }

    impl PowerProfileAdapter for TestPowerProfileAdapter {
        fn spawn(
            mut self,
            tx: watch::Sender<PowerProfileInfo>,
            mut command_rx: mpsc::UnboundedReceiver<PowerProfileCommand>,
        ) -> JoinHandle<()> {
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        command = command_rx.recv() => {
                            match command {
                                Some(PowerProfileCommand::Set(profile)) => {
                                    let _ = tx.send_replace(PowerProfileInfo::live(profile));
                                }
                                None => return,
                            }
                        }
                        event = self.event_rx.recv() => {
                            match event {
                                Some(info) => {
                                    let _ = tx.send_replace(info);
                                }
                                None => return,
                            }
                        }
                    }
                }
            })
        }
    }
}
