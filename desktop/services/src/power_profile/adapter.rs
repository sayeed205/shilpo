//! Backend adapters for the power profile service.
//!
//! This module hides the system IPC (D-Bus) protocol behind a narrow seam so
//! the rest of the application never touches `zbus`. Each adapter owns an
//! async event loop that watches the daemon for `PropertiesChanged` signals
//! and forwards the updated state to the harness via a `watch::Sender`.

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::{PowerProfileCommand, PowerProfileInfo};

/// Channels handed to a backend when it is spawned.
///
/// The state sender lets the backend publish state; the command receiver lets
/// the harness dispatch profile changes.
use super::{PowerProfileCommand, PowerProfileInfo};
use crate::runtime::CommandContext;

/// Seam between the harness and a concrete power profile backend.
pub(crate) trait PowerProfileAdapter: Send + 'static {
    /// Run the backend event loop. The loop owns `ctx` and runs until
    /// the service runtime is dropped.
    fn run(
        self,
        ctx: CommandContext<PowerProfileInfo, PowerProfileCommand>,
    ) -> impl std::future::Future<Output = ()> + Send + 'static;
}

#[cfg(target_os = "linux")]
pub(crate) use dbus::ZbusPowerProfileAdapter;

#[cfg(target_os = "linux")]
pub(crate) mod dbus {
    //! D-Bus backend backed by the `net.hadess.PowerProfiles` daemon.

    use std::collections::HashMap;
    use std::time::Duration;

    use futures_lite::StreamExt;

    use super::PowerProfileAdapter;
    use crate::power_profile::{PowerProfile, PowerProfileCommand, PowerProfileInfo};
    use crate::runtime::{CommandContext, StateContext};

    /// The system bus name, object path, and interface of the daemon.
    const POWER_PROFILES_BUS: &str = "net.hadess.PowerProfiles";
    const POWER_PROFILES_PATH: &str = "/net/hadess/PowerProfiles";
    const POWER_PROFILES_IFACE: &str = "net.hadess.PowerProfiles";
    const RECONNECT_DELAY: Duration = Duration::from_secs(2);

    /// Property names exposed by the daemon.
    const ACTIVE_PROFILE_PROP: &str = "ActiveProfile";
    const PROFILES_PROP: &str = "Profiles";

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

    /// Everything the event loop needs from a live connection to the daemon.
    struct DaemonSession {
        /// Held for the connection's lifetime; dropping it closes the bus.
        _connection: zbus::Connection,
        proxy: PowerProfilesProxy<'static>,
        /// Held so the property-change subscription stays name-owned.
        _properties: zbus::fdo::PropertiesProxy<'static>,
        changes: zbus::fdo::PropertiesChangedStream,
    }

    /// D-Bus backed adapter for the power profiles daemon.
    pub(crate) struct ZbusPowerProfileAdapter;

    impl PowerProfileAdapter for ZbusPowerProfileAdapter {
        async fn run(self, ctx: CommandContext<PowerProfileInfo, PowerProfileCommand>) {
            run_loop(ctx).await;
        }
    }

    async fn connect() -> zbus::Result<DaemonSession> {
        let connection = zbus::Connection::system().await?;
        let proxy = PowerProfilesProxy::new(&connection).await?;
        let properties = zbus::fdo::PropertiesProxy::builder(&connection)
            .destination(POWER_PROFILES_BUS)?
            .path(POWER_PROFILES_PATH)?
            .build()
            .await?;
        let changes = properties.receive_properties_changed().await?;
        Ok(DaemonSession {
            _connection: connection,
            proxy,
            _properties: properties,
            changes,
        })
    }

    /// Whether a `PropertiesChanged` payload affects the active power profile.
    fn is_relevant_change(
        interface_name: &str,
        changed: &HashMap<&str, zbus::zvariant::Value<'_>>,
        invalidated: &[&str],
    ) -> bool {
        interface_name == POWER_PROFILES_IFACE
            && (changed.contains_key(ACTIVE_PROFILE_PROP)
                || changed.contains_key(PROFILES_PROP)
                || invalidated
                    .iter()
                    .any(|prop| *prop == ACTIVE_PROFILE_PROP || *prop == PROFILES_PROP))
    }

    async fn run_loop(mut ctx: CommandContext<PowerProfileInfo, PowerProfileCommand>) {
        loop {
            let session = match connect().await {
                Ok(session) => session,
                Err(error) => {
                    let msg =
                        format!("failed to initialize power profile D-Bus connection: {error}");
                    reconnect(&ctx.state, Some(&msg)).await;
                    continue;
                }
            };

            if let Ok(active) = session.proxy.active_profile().await {
                ctx.state.send_replace(PowerProfileInfo::from_daemon(&active));
            } else {
                ctx.state.send_replace(PowerProfileInfo::fallback());
            }

            let mut changes = session.changes;

            loop {
                tokio::select! {
                    command = ctx.command_rx.recv() => {
                        match command {
                            Some(PowerProfileCommand::Set(profile)) => {
                                apply_set(&session.proxy, &ctx.state, profile).await;
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
                        if is_relevant_change(
                            args.interface_name.as_str(),
                            &args.changed_properties,
                            &args.invalidated_properties,
                        ) && let Ok(active) = session.proxy.active_profile().await
                        {
                            ctx.state.send_replace(PowerProfileInfo::from_daemon(&active));
                        }
                    }
                }
            }

            reconnect(&ctx.state, None).await;
        }
    }

    /// Applies a `Set` command over D-Bus and publishes the confirmed state.
    ///
    /// On failure the daemon's current profile is re-read so subscribers are
    /// reconciled with reality instead of trusting the rejected request.
    async fn apply_set(
        proxy: &PowerProfilesProxy<'_>,
        state: &StateContext<PowerProfileInfo>,
        profile: PowerProfile,
    ) {
        if proxy.set_active_profile(profile.as_str()).await.is_ok() {
            state.send_replace(PowerProfileInfo::online(profile));
        } else if let Ok(active) = proxy.active_profile().await {
            state.send_replace(PowerProfileInfo::from_daemon(&active));
        }
    }

    /// Marks the daemon offline and waits before the next reconnect attempt.
    async fn reconnect(state: &StateContext<PowerProfileInfo>, reason: Option<&str>) {
        if let Some(msg) = reason {
            tracing::warn!("power profile: {msg}; reconnecting");
        } else {
            tracing::debug!("power profile: system bus connection lost; reconnecting");
        }
        state.send_replace(PowerProfileInfo::fallback());
        tokio::time::sleep(RECONNECT_DELAY).await;
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn changed_map<'a>(props: &'a [&'a str]) -> HashMap<&'a str, zbus::zvariant::Value<'a>> {
            props
                .iter()
                .map(|prop| (*prop, zbus::zvariant::Value::from("value")))
                .collect()
        }

        #[test]
        fn test_is_relevant_change_filters_signals() {
            let empty = HashMap::new();
            let iface = POWER_PROFILES_IFACE;

            assert!(is_relevant_change(
                iface,
                &changed_map(&["ActiveProfile"]),
                &[]
            ));
            assert!(is_relevant_change(iface, &changed_map(&["Profiles"]), &[]));
            assert!(is_relevant_change(iface, &empty, &["ActiveProfile"]));
            assert!(is_relevant_change(iface, &empty, &["Profiles"]));

            assert!(!is_relevant_change(iface, &changed_map(&["Version"]), &[]));
            assert!(!is_relevant_change(
                "org.freedesktop.Notifications",
                &changed_map(&["ActiveProfile"]),
                &[]
            ));
            assert!(!is_relevant_change(iface, &empty, &[]));
        }
    }
}

#[cfg(test)]
pub(crate) mod mock {
    //! In-memory backend that simulates `PropertiesChanged` events for tests.

    use tokio::sync::mpsc;

    use super::PowerProfileAdapter;
    use crate::power_profile::{PowerProfileCommand, PowerProfileInfo};
    use crate::runtime::CommandContext;

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
        async fn run(mut self, mut ctx: CommandContext<PowerProfileInfo, PowerProfileCommand>) {
            loop {
                tokio::select! {
                    command = ctx.command_rx.recv() => {
                        match command {
                            Some(PowerProfileCommand::Set(profile)) => {
                                ctx.state.send_replace(PowerProfileInfo::online(profile));
                            }
                            None => return,
                        }
                    }
                    event = self.event_rx.recv() => {
                        match event {
                            Some(info) => {
                                ctx.state.send_replace(info);
                            }
                            None => return,
                        }
                    }
                }
            }
        }
    }
}

