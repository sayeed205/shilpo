use std::sync::{Arc, Mutex};

use shilpo_ui::theme::{ColorSource, SchemeVariant, ThemeMode};
use tokio::sync::mpsc;
use zbus::object_server::SignalEmitter;

use crate::{daemon::DaemonState, executors::ProjectionStatus};

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
    actor_tx: mpsc::UnboundedSender<ActorMessage>,
    effects: Arc<Mutex<EffectStatus>>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct EffectStatus {
    pub durable_revision: u64,
    pub projection_status: ProjectionStatus,
}

impl ThemeDbusService {
    pub fn new(
        actor_tx: mpsc::UnboundedSender<ActorMessage>,
        effects: Arc<Mutex<EffectStatus>>,
    ) -> Self {
        Self { actor_tx, effects }
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
        let _ = self.actor_tx.send(ActorMessage::GetState(tx));
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn get_diagnostics(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = self.actor_tx.send(ActorMessage::GetDiagnostics(tx));
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
        self.actor_tx
            .send(ActorMessage::SetMode(mode, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn toggle_mode(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::ToggleMode(tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_color_source(&self, source_str: String) -> zbus::fdo::Result<String> {
        let source = serde_json::from_str(&format!("\"{source_str}\""))
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetColorSource(source, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_scheme_variant(&self, variant_str: String) -> zbus::fdo::Result<String> {
        let variant = SchemeVariant::from_str(&variant_str);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetSchemeVariant(variant, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_custom_seed(&self, argb: u32) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetCustomSeed(argb, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_wallpaper(&self, path: String) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetWallpaper(path, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_wallpaper_directory(&self, dir: String) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetWallpaperDirectory(dir, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        self.state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_random_wallpaper(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetRandomWallpaper(tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
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
