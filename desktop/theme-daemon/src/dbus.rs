use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use shilpo_m3e::theme::{ColorSource, SchemeVariant, ThemeMode};
use tokio::sync::mpsc;
use zbus::object_server::SignalEmitter;

use crate::daemon::DaemonState;
use crate::executors::ProjectionStatus;

/// Bounded capacity for the actor mailbox.
pub const ACTOR_MAILBOX_CAPACITY: usize = 32;

pub enum ActorMessage {
    GetState(tokio::sync::oneshot::Sender<Result<DaemonState, String>>),
    GetDiagnostics(tokio::sync::oneshot::Sender<String>),
    SetMode(
        ThemeMode,
        tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
    ),
    ToggleMode(tokio::sync::oneshot::Sender<Result<DaemonState, String>>),
    SetColorSource(
        ColorSource,
        tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
    ),
    SetSchemeVariant(
        SchemeVariant,
        tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
    ),
    SetCustomSeed(
        u32,
        tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
    ),
    SetWallpaper(
        String,
        tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
    ),
    SetWallpaperDirectory(
        String,
        tokio::sync::oneshot::Sender<Result<DaemonState, String>>,
    ),
    SetRandomWallpaper(tokio::sync::oneshot::Sender<Result<DaemonState, String>>),
}

pub struct ThemeDbusService {
    actor_tx: mpsc::Sender<ActorMessage>,
    effects: Arc<Mutex<EffectStatus>>,
    actor_overloads: Arc<AtomicU64>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct EffectStatus {
    pub durable_revision: u64,
    pub projection_status: ProjectionStatus,
}

impl ThemeDbusService {
    pub fn new(
        actor_tx: mpsc::Sender<ActorMessage>,
        effects: Arc<Mutex<EffectStatus>>,
        actor_overloads: Arc<AtomicU64>,
    ) -> Self {
        Self {
            actor_tx,
            effects,
            actor_overloads,
        }
    }

    /// Try-send a message to the actor and map `MailboxError` to a D-Bus fault.
    fn try_send(&self, msg: ActorMessage) -> zbus::fdo::Result<()> {
        match self.actor_tx.try_send(msg) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.actor_overloads.fetch_add(1, Ordering::SeqCst);
                tracing::warn!(
                    site = "ThemeDbusService",
                    policy = "Lossless",
                    capacity = ACTOR_MAILBOX_CAPACITY,
                    "actor mailbox full; D-Bus request rejected"
                );
                Err(zbus::fdo::Error::Failed(
                    "Theme daemon is overloaded; retry".into(),
                ))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!(site = "ThemeDbusService", "actor mailbox closed");
                Err(zbus::fdo::Error::Failed("Actor connection closed".into()))
            }
        }
    }

    fn state_result_to_json(
        &self,
        result: Result<DaemonState, String>,
    ) -> zbus::fdo::Result<String> {
        match result {
            Ok(state) => {
                let mut value = serde_json::to_value(&state)
                    .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
                if let Some(object) = value.as_object_mut() {
                    let status = self.effects.lock().unwrap().clone();
                    object.insert(
                        "committed_revision".into(),
                        serde_json::json!(state.theme.revision),
                    );
                    object.insert(
                        "durable_revision".into(),
                        serde_json::json!(status.durable_revision),
                    );
                    object.insert(
                        "projection_status".into(),
                        serde_json::to_value(status.projection_status).unwrap_or_default(),
                    );
                }
                serde_json::to_string(&value)
                    .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
            }
            Err(error) => Err(zbus::fdo::Error::Failed(error)),
        }
    }
}

#[zbus::interface(name = "org.shilpo.Theme")]
impl ThemeDbusService {
    async fn get_state(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::GetState(tx))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn get_diagnostics(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::GetDiagnostics(tx))?;
        rx.await
            .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))
    }

    async fn get_health(&self) -> zbus::fdo::Result<String> {
        self.get_diagnostics().await
    }

    async fn set_mode(&self, mode_str: String) -> zbus::fdo::Result<String> {
        let mode = serde_json::from_str(&format!("\"{mode_str}\""))
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::SetMode(mode, tx))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn toggle_mode(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::ToggleMode(tx))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_color_source(&self, source_str: String) -> zbus::fdo::Result<String> {
        let source = serde_json::from_str(&format!("\"{source_str}\""))
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::SetColorSource(source, tx))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_scheme_variant(&self, variant_str: String) -> zbus::fdo::Result<String> {
        let variant = SchemeVariant::from_str(&variant_str);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::SetSchemeVariant(variant, tx))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_custom_seed(&self, argb: u32) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::SetCustomSeed(argb, tx))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_wallpaper(&self, path: String) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::SetWallpaper(path, tx))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_wallpaper_directory(&self, dir: String) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::SetWallpaperDirectory(dir, tx))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_random_wallpaper(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.try_send(ActorMessage::SetRandomWallpaper(tx))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    #[zbus(signal)]
    pub async fn state_changed(
        signal_emitter: &SignalEmitter<'_>,
        state: &str,
    ) -> zbus::Result<()> {
    }
}

#[zbus::proxy(
    interface = "org.shilpo.Theme",
    default_service = "org.shilpo.Theme",
    default_path = "/org/shilpo/Theme"
)]
pub trait ThemeDbus {
    fn get_state(&self) -> zbus::Result<String>;
    fn get_diagnostics(&self) -> zbus::Result<String>;
    fn get_health(&self) -> zbus::Result<String>;
    fn set_mode(&self, mode: &str) -> zbus::Result<String>;
    fn toggle_mode(&self) -> zbus::Result<String>;
    fn set_color_source(&self, source: &str) -> zbus::Result<String>;
    fn set_scheme_variant(&self, variant: &str) -> zbus::Result<String>;
    fn set_custom_seed(&self, argb: u32) -> zbus::Result<String>;
    fn set_wallpaper(&self, path: &str) -> zbus::Result<String>;
    fn set_wallpaper_directory(&self, dir: &str) -> zbus::Result<String>;
    fn set_random_wallpaper(&self) -> zbus::Result<String>;

    #[zbus(signal)]
    fn state_changed(&self, state: String) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_effects() -> Arc<Mutex<EffectStatus>> {
        Arc::new(Mutex::new(EffectStatus {
            durable_revision: 0,
            projection_status: ProjectionStatus::Applied,
        }))
    }

    // actor_tx overload, driven through the real production `try_send` path.
    //
    // ThemeDbusService and ThemeDaemon hold the sender and receiver of this
    // channel independently (unlike PersistenceExecutor/AdapterExecutor, which
    // each own both halves), so filling the channel here and calling a real
    // `#[zbus::interface]` method exercises exactly the path a live D-Bus
    // caller would hit under sustained pressure.
    #[tokio::test]
    async fn test_actor_mailbox_overload() {
        let (tx, _rx) = mpsc::channel(ACTOR_MAILBOX_CAPACITY);
        let overloads = Arc::new(AtomicU64::new(0));
        let service = ThemeDbusService::new(tx.clone(), test_effects(), overloads.clone());

        // Fill to capacity via a raw clone; nothing drains `_rx` in this test.
        for i in 0..ACTOR_MAILBOX_CAPACITY {
            let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
            tx.try_send(ActorMessage::GetDiagnostics(reply_tx))
                .unwrap_or_else(|_| {
                    panic!("slot {i} should fit in capacity {ACTOR_MAILBOX_CAPACITY}")
                });
        }

        // The next request goes through the real `get_diagnostics` D-Bus method,
        // which calls the same `try_send` production path.
        let result = service.get_diagnostics().await;
        match result {
            Err(zbus::fdo::Error::Failed(msg)) => {
                assert!(
                    msg.contains("overloaded"),
                    "expected an overload message, got: {msg}"
                );
            }
            other => panic!("expected Err(Failed(..)) on overload, got {other:?}"),
        }
        assert_eq!(overloads.load(Ordering::SeqCst), 1);
    }

    // actor_tx closed, driven through the real production `try_send` path.
    #[tokio::test]
    async fn test_actor_mailbox_closed() {
        let (tx, rx) = mpsc::channel(ACTOR_MAILBOX_CAPACITY);
        // ThemeDaemon's actor_rx has been dropped (e.g. mid-shutdown); the
        // D-Bus service is still alive and holding its sender.
        drop(rx);

        let overloads = Arc::new(AtomicU64::new(0));
        let service = ThemeDbusService::new(tx, test_effects(), overloads.clone());

        let result = service.get_state().await;
        match result {
            Err(zbus::fdo::Error::Failed(msg)) => {
                assert!(
                    msg.contains("Actor connection closed"),
                    "expected the closed-channel message, got: {msg}"
                );
            }
            other => panic!("expected Err(Failed(..)) on closed channel, got {other:?}"),
        }
        // Closed is not an overload — the counter must not move.
        assert_eq!(overloads.load(Ordering::SeqCst), 0);
    }
}
