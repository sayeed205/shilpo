use shilpo_theme::{ColorSource, SchemeVariant, ThemeMode, ThemeState};
use tokio::sync::mpsc;
use zbus::object_server::SignalEmitter;

pub enum ActorMessage {
    GetState(tokio::sync::oneshot::Sender<Result<ThemeState, String>>),
    GetDiagnostics(tokio::sync::oneshot::Sender<String>),
    SetMode(
        ThemeMode,
        tokio::sync::oneshot::Sender<Result<ThemeState, String>>,
    ),
    ToggleMode(tokio::sync::oneshot::Sender<Result<ThemeState, String>>),
    SetColorSource(
        ColorSource,
        tokio::sync::oneshot::Sender<Result<ThemeState, String>>,
    ),
    SetSchemeVariant(
        SchemeVariant,
        tokio::sync::oneshot::Sender<Result<ThemeState, String>>,
    ),
    SetCustomSeed(
        u32,
        tokio::sync::oneshot::Sender<Result<ThemeState, String>>,
    ),
    SetWallpaper(
        String,
        tokio::sync::oneshot::Sender<Result<ThemeState, String>>,
    ),
    SetWallpaperDirectory(
        String,
        tokio::sync::oneshot::Sender<Result<ThemeState, String>>,
    ),
    SetRandomWallpaper(tokio::sync::oneshot::Sender<Result<ThemeState, String>>),
}

pub struct ThemeDbusService {
    actor_tx: mpsc::UnboundedSender<ActorMessage>,
}

impl ThemeDbusService {
    pub fn new(actor_tx: mpsc::UnboundedSender<ActorMessage>) -> Self {
        Self { actor_tx }
    }
}

#[zbus::interface(name = "org.shilpo.Theme")]
impl ThemeDbusService {
    async fn get_state(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = self.actor_tx.send(ActorMessage::GetState(tx));
        state_result_to_json(
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

    async fn set_mode(&self, mode_str: String) -> zbus::fdo::Result<String> {
        let mode = serde_json::from_str(&format!("\"{mode_str}\""))
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetMode(mode, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn toggle_mode(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::ToggleMode(tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        state_result_to_json(
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
        state_result_to_json(
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
        state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_custom_seed(&self, argb: u32) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetCustomSeed(argb, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_wallpaper(&self, path: String) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetWallpaper(path, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_wallpaper_directory(&self, dir: String) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetWallpaperDirectory(dir, tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        state_result_to_json(
            rx.await
                .map_err(|_| zbus::fdo::Error::Failed("Actor request dropped".into()))?,
        )
    }

    async fn set_random_wallpaper(&self) -> zbus::fdo::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.actor_tx
            .send(ActorMessage::SetRandomWallpaper(tx))
            .map_err(|_| zbus::fdo::Error::Failed("Actor connection closed".into()))?;
        state_result_to_json(
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

fn state_result_to_json(result: Result<ThemeState, String>) -> zbus::fdo::Result<String> {
    match result {
        Ok(state) => serde_json::to_string(&state)
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string())),
        Err(error) => Err(zbus::fdo::Error::Failed(error)),
    }
}
