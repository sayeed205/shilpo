use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use shilpo_domain::{DomainPortTelemetry, MonotonicTimeSource, SupervisorState, TimeSource};
use tokio::sync::watch;
use zbus::Connection;
use zbus::zvariant::Value;

use super::agent::{AuthorityClient, POLKIT_AGENT_OBJECT_PATH, PolkitAgentServer};
use super::helper::{PolkitHelper, SystemPolkitHelper};
use super::state::PolkitDomainState;
use super::types::{
    CommandTicket, PolkitCommand, PolkitCommandOutcome, PolkitPort, PolkitSnapshot,
};

type SubjectCell = Arc<Mutex<Option<(String, HashMap<String, Value<'static>>)>>>;

/// Service wrapper for the PolicyKit authentication agent domain.
pub struct PolkitService {
    adapter: Arc<PolkitDomainState>,
    connection: Arc<Mutex<Option<Connection>>>,
    registered_subject: SubjectCell,
    time_source: Arc<dyn TimeSource>,
}

impl PolkitService {
    /// Creates and starts a new `PolkitService` on the system D-Bus.
    pub fn new() -> Result<Self> {
        Self::with_helper(Arc::new(SystemPolkitHelper::new()))
    }

    /// Creates and starts a new `PolkitService` with a custom helper implementation.
    pub fn with_helper(helper: Arc<dyn PolkitHelper>) -> Result<Self> {
        let time_source: Arc<dyn TimeSource> = Arc::new(MonotonicTimeSource::new());
        let adapter = Arc::new(PolkitDomainState::with_time_source(
            32,
            helper,
            time_source.clone(),
            super::state::DEFAULT_INACTIVITY_TIMEOUT_MS,
        ));
        let connection = Arc::new(Mutex::new(None));
        let registered_subject = Arc::new(Mutex::new(None));

        let service = Self {
            adapter: adapter.clone(),
            connection: connection.clone(),
            registered_subject: registered_subject.clone(),
            time_source: time_source.clone(),
        };

        service.spawn_supervisor(adapter, connection, registered_subject, time_source);
        Ok(service)
    }

    /// Creates an offline `PolkitService` suitable for tests without D-Bus.
    pub fn new_offline(helper: Arc<dyn PolkitHelper>) -> Self {
        let time_source: Arc<dyn TimeSource> = Arc::new(MonotonicTimeSource::new());
        let adapter = Arc::new(PolkitDomainState::with_time_source(
            32,
            helper,
            time_source.clone(),
            super::state::DEFAULT_INACTIVITY_TIMEOUT_MS,
        ));
        Self {
            adapter,
            connection: Arc::new(Mutex::new(None)),
            registered_subject: Arc::new(Mutex::new(None)),
            time_source,
        }
    }

    /// Creates an offline ready `PolkitService` for tests.
    pub fn new_ready_for_test(helper: Arc<dyn PolkitHelper>) -> Self {
        let time_source: Arc<dyn TimeSource> = Arc::new(MonotonicTimeSource::new());
        let adapter = Arc::new(PolkitDomainState::with_time_source(
            32,
            helper,
            time_source.clone(),
            super::state::DEFAULT_INACTIVITY_TIMEOUT_MS,
        ));
        adapter.begin_start();
        adapter.mark_ready(time_source.now_ms());
        Self {
            adapter,
            connection: Arc::new(Mutex::new(None)),
            registered_subject: Arc::new(Mutex::new(None)),
            time_source,
        }
    }

    fn spawn_supervisor(
        &self,
        adapter: Arc<PolkitDomainState>,
        connection_slot: Arc<Mutex<Option<Connection>>>,
        subject_slot: SubjectCell,
        time_source: Arc<dyn TimeSource>,
    ) {
        tokio::spawn(async move {
            let mut connection: Option<Connection> = None;
            let mut registered_subj: Option<(String, HashMap<String, Value<'static>>)> = None;

            loop {
                let supervisor_state = adapter.supervisor_state();
                let now_ms = time_source.now_ms();

                match supervisor_state {
                    SupervisorState::Running => {
                        // Check if D-Bus connection is still alive
                        let is_closed = connection.as_ref().map(|c| c.is_closed()).unwrap_or(true);
                        if is_closed {
                            adapter.report_owner_failure(
                                "polkit system D-Bus connection lost".into(),
                                now_ms,
                            );
                            *connection_slot.lock().unwrap() = None;
                            *subject_slot.lock().unwrap() = None;
                            connection = None;
                            registered_subj = None;
                        } else {
                            adapter.tick(now_ms);
                            // Poll any helper events if active
                            adapter.poll_active_helper_event();
                        }
                    }
                    SupervisorState::Backoff { .. } => {
                        adapter.tick(now_ms);
                    }
                    SupervisorState::Quarantined => {
                        tokio::time::sleep(Duration::from_millis(1_000)).await;
                        let now_ms = time_source.now_ms();
                        adapter.tick(now_ms);
                    }
                    SupervisorState::Starting => {
                        *connection_slot.lock().unwrap() = None;
                        *subject_slot.lock().unwrap() = None;
                        connection = None;
                        registered_subj = None;

                        // Connect to system bus
                        let next_conn = match Connection::system().await {
                            Ok(c) => c,
                            Err(err) => {
                                let now_ms = time_source.now_ms();
                                adapter.report_owner_failure(
                                    format!("failed to connect to system bus: {err}"),
                                    now_ms,
                                );
                                tokio::time::sleep(Duration::from_millis(250)).await;
                                continue;
                            }
                        };

                        // Register AuthenticationAgent interface on object server
                        let server = PolkitAgentServer::new(adapter.clone());
                        if let Err(err) = next_conn
                            .object_server()
                            .at(POLKIT_AGENT_OBJECT_PATH, server)
                            .await
                        {
                            let now_ms = time_source.now_ms();
                            adapter.report_owner_failure(
                                format!("failed to register Polkit agent object: {err}"),
                                now_ms,
                            );
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            continue;
                        }

                        // Call Authority.RegisterAuthenticationAgent
                        let reg_res =
                            AuthorityClient::register_agent(&next_conn, POLKIT_AGENT_OBJECT_PATH)
                                .await;

                        match reg_res {
                            Ok(subject) => {
                                adapter.begin_start();
                                let now_ms = time_source.now_ms();
                                adapter.mark_ready(now_ms);
                                *connection_slot.lock().unwrap() = Some(next_conn.clone());
                                *subject_slot.lock().unwrap() = Some(subject.clone());
                                connection = Some(next_conn);
                                registered_subj = Some(subject);
                            }
                            Err(err) => {
                                let now_ms = time_source.now_ms();
                                adapter.report_owner_failure(
                                    format!("failed to register with polkit Authority: {err}"),
                                    now_ms,
                                );
                                tokio::time::sleep(Duration::from_millis(250)).await;
                                continue;
                            }
                        }
                    }
                    SupervisorState::Stopping | SupervisorState::Stopped => {
                        if let (Some(conn), Some(subj)) =
                            (connection.as_ref(), registered_subj.as_ref())
                        {
                            let _ = AuthorityClient::unregister_agent(
                                conn,
                                subj,
                                POLKIT_AGENT_OBJECT_PATH,
                            )
                            .await;
                        }
                        break;
                    }
                }

                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });
    }

    pub fn is_dbus_connected(&self) -> bool {
        self.connection.lock().unwrap().is_some()
    }

    pub fn is_healthy(&self) -> bool {
        self.connection
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|c| !c.is_closed())
    }

    pub fn time_source(&self) -> &Arc<dyn TimeSource> {
        &self.time_source
    }

    pub fn adapter(&self) -> &Arc<PolkitDomainState> {
        &self.adapter
    }

    pub fn shutdown(&self) {
        self.adapter.shutdown();
        let conn = self.connection.lock().unwrap().take();
        let subj = self.registered_subject.lock().unwrap().take();
        if let (Some(conn), Some(subj)) = (conn, subj) {
            tokio::spawn(async move {
                let _ =
                    AuthorityClient::unregister_agent(&conn, &subj, POLKIT_AGENT_OBJECT_PATH).await;
            });
        }
    }
}

impl Drop for PolkitService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl PolkitPort for PolkitService {
    fn snapshot(&self) -> PolkitSnapshot {
        self.adapter.snapshot()
    }

    fn subscribe(&self) -> watch::Receiver<PolkitSnapshot> {
        self.adapter.subscribe()
    }

    fn submit_command(
        &self,
        command: PolkitCommand,
    ) -> Result<CommandTicket, PolkitCommandOutcome> {
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

    fn shutdown(&self) {
        self.shutdown();
    }
}
