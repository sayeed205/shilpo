use std::sync::Arc;
use std::time::Duration;

use futures_lite::StreamExt;
use tokio::sync::{mpsc, watch};
use zbus::fdo::DBusProxy;
use zbus::names::WellKnownName;
use zbus::{Connection, proxy};

use super::actions::{IdleActionSink, SystemIdleActionSink};
use super::backend::{
    IdleBackendEvent, IdleNotifierBackend, MockIdleNotifier, WaylandIdleNotifier,
};
use super::inhibits::ScreenSaverServer;
use super::state::IdleDomainState;
use super::types::{
    CommandTicket, DomainPortTelemetry, IdleCommand, IdleCommandOutcome, IdlePort, IdleSnapshot,
    InhibitSource, SupervisorState, TimeSource,
};

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
pub trait LogindManager {
    #[zbus(property)]
    fn block_inhibited(&self) -> zbus::Result<String>;
}

/// Service wrapper managing the lifecycle, D-Bus interfaces, and event loop for the Idle domain.
pub struct IdleService {
    adapter: Arc<IdleDomainState>,
    _cmd_tx: mpsc::UnboundedSender<IdleCommand>,
    time_source: Arc<dyn TimeSource>,
}

impl Default for IdleService {
    fn default() -> Self {
        Self::new()
    }
}

impl IdleService {
    /// Creates and starts a production `IdleService` with real Wayland and D-Bus backends.
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let backend: Arc<dyn IdleNotifierBackend> = match WaylandIdleNotifier::new(event_tx) {
            Ok(wayland) => Arc::new(wayland),
            Err(err) => {
                tracing::warn!(%err, "wayland idle notifier backend unavailable; using mock fallback");
                Arc::new(MockIdleNotifier::new())
            }
        };

        let action_sink: Arc<dyn IdleActionSink> = Arc::new(SystemIdleActionSink::new(None));
        let time_source: Arc<dyn TimeSource> = Arc::new(shilpo_domain::MonotonicTimeSource::new());

        Self::with_components(backend, action_sink, time_source, event_rx)
    }

    /// Creates an offline `IdleService` for tests without live Wayland or D-Bus connections.
    pub fn new_offline(
        backend: Arc<dyn IdleNotifierBackend>,
        action_sink: Arc<dyn IdleActionSink>,
    ) -> Self {
        let (_event_tx, event_rx) = mpsc::unbounded_channel::<IdleBackendEvent>();
        let time_source: Arc<dyn TimeSource> = Arc::new(shilpo_domain::MonotonicTimeSource::new());
        Self::with_components(backend, action_sink, time_source, event_rx)
    }

    /// Creates a mock ready `IdleService` for test environments.
    pub fn new_mock() -> Self {
        Self::new_ready_for_test(
            Arc::new(crate::idle::backend::MockIdleNotifier::new()),
            Arc::new(crate::idle::actions::MockIdleActionSink::new()),
        )
    }

    /// Creates an offline ready `IdleService` for test environments.
    pub fn new_ready_for_test(
        backend: Arc<dyn IdleNotifierBackend>,
        action_sink: Arc<dyn IdleActionSink>,
    ) -> Self {
        let time_source: Arc<dyn TimeSource> = Arc::new(shilpo_domain::MonotonicTimeSource::new());
        let adapter = Arc::new(IdleDomainState::new(
            32,
            backend,
            action_sink,
            time_source.clone(),
        ));
        adapter.begin_start();
        adapter.mark_ready(time_source.now_ms());

        let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
        Self {
            adapter,
            _cmd_tx: cmd_tx,
            time_source,
        }
    }

    pub fn with_components(
        backend: Arc<dyn IdleNotifierBackend>,
        action_sink: Arc<dyn IdleActionSink>,
        time_source: Arc<dyn TimeSource>,
        event_rx: mpsc::UnboundedReceiver<IdleBackendEvent>,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let adapter = Arc::new(IdleDomainState::new(
            32,
            backend,
            action_sink,
            time_source.clone(),
        ));

        let service = Self {
            adapter: adapter.clone(),
            _cmd_tx: cmd_tx.clone(),
            time_source: time_source.clone(),
        };

        Self::spawn_supervisor(adapter, cmd_tx, cmd_rx, event_rx, time_source);
        service
    }

    pub fn adapter(&self) -> Arc<IdleDomainState> {
        self.adapter.clone()
    }

    pub fn time_source(&self) -> Arc<dyn TimeSource> {
        self.time_source.clone()
    }

    fn spawn_supervisor(
        adapter: Arc<IdleDomainState>,
        cmd_tx: mpsc::UnboundedSender<IdleCommand>,
        mut cmd_rx: mpsc::UnboundedReceiver<IdleCommand>,
        mut event_rx: mpsc::UnboundedReceiver<IdleBackendEvent>,
        time_source: Arc<dyn TimeSource>,
    ) {
        tokio::spawn(async move {
            adapter.begin_start();

            // Attempt session D-Bus registration
            let session_conn = Connection::session().await.ok();
            if let Some(ref conn) = session_conn {
                let adapter_clone = adapter.clone();
                let server = ScreenSaverServer::new(
                    cmd_tx.clone(),
                    Arc::new(move || adapter_clone.snapshot().live_idle_seconds),
                );

                let _ = conn
                    .object_server()
                    .at("/org/freedesktop/ScreenSaver", server)
                    .await;

                let adapter_clone2 = adapter.clone();
                let server2 = ScreenSaverServer::new(
                    cmd_tx.clone(),
                    Arc::new(move || adapter_clone2.snapshot().live_idle_seconds),
                );
                let _ = conn.object_server().at("/ScreenSaver", server2).await;

                // Request name org.freedesktop.ScreenSaver
                match conn
                    .request_name(
                        WellKnownName::from_static_str("org.freedesktop.ScreenSaver").unwrap(),
                    )
                    .await
                {
                    Ok(reply) => {
                        tracing::info!(?reply, "registered org.freedesktop.ScreenSaver");
                    }
                    Err(err) => {
                        tracing::warn!(%err, "could not acquire org.freedesktop.ScreenSaver name");
                    }
                }

                // Subscribe to NameOwnerChanged to release cookies when clients drop
                let conn_clone = conn.clone();
                let cmd_tx_clone = cmd_tx.clone();
                tokio::spawn(async move {
                    if let Ok(dbus_proxy) = DBusProxy::new(&conn_clone).await
                        && let Ok(mut stream) = dbus_proxy.receive_name_owner_changed().await
                    {
                        while let Some(sig) = stream.next().await {
                            if let Ok(args) = sig.args()
                                && (args.new_owner.is_none()
                                    || args.new_owner.as_deref() == Some(""))
                            {
                                let _ = cmd_tx_clone.send(IdleCommand::ClearInhibitsForSender {
                                    sender: args.name.to_string(),
                                });
                            }
                        }
                    }
                });
            }

            // Watch system logind BlockInhibited property
            let system_conn = Connection::system().await.ok();
            if let Some(ref sys_conn) = system_conn {
                let cmd_tx_clone = cmd_tx.clone();
                let sys_conn_clone = sys_conn.clone();
                tokio::spawn(async move {
                    if let Ok(manager_proxy) = LogindManagerProxy::new(&sys_conn_clone).await {
                        let mut prop_stream = manager_proxy.receive_block_inhibited_changed().await;
                        while let Some(prop) = prop_stream.next().await {
                            if let Ok(val) = prop.get().await {
                                if val.contains("idle") {
                                    let _ = cmd_tx_clone.send(IdleCommand::AddInhibit {
                                        source: InhibitSource::LogindBlockInhibited,
                                    });
                                } else {
                                    let _ = cmd_tx_clone.send(IdleCommand::RemoveInhibit {
                                        source: InhibitSource::LogindBlockInhibited,
                                    });
                                }
                            }
                        }
                    }
                });
            }

            adapter.mark_ready(time_source.now_ms());

            let mut tick_interval = tokio::time::interval(Duration::from_millis(500));

            loop {
                tokio::select! {
                    _ = tick_interval.tick() => {
                        adapter.tick(time_source.now_ms());
                    }
                    Some(event) = event_rx.recv() => {
                        adapter.handle_backend_event(event);
                    }
                    Some(cmd) = cmd_rx.recv() => {
                        let _ = adapter.submit_command(cmd);
                    }
                }
            }
        });
    }
}

impl IdlePort for IdleService {
    fn snapshot(&self) -> IdleSnapshot {
        self.adapter.snapshot()
    }

    fn subscribe(&self) -> watch::Receiver<IdleSnapshot> {
        self.adapter.subscribe()
    }

    fn submit_command(&self, command: IdleCommand) -> Result<CommandTicket, IdleCommandOutcome> {
        self.adapter.submit_command(command)
    }

    fn supervisor_state(&self) -> SupervisorState {
        self.adapter.supervisor_state()
    }

    fn telemetry(&self) -> DomainPortTelemetry {
        self.adapter.telemetry()
    }

    fn reset_quarantine(&self) {
        self.adapter.reset_quarantine();
    }
}
