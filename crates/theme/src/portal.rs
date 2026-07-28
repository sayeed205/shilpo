use crate::state::ThemeMode;
use ashpd::desktop::settings::{ColorScheme, Settings};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

pub struct PortalObserver;

impl PortalObserver {
    pub async fn start(tx: mpsc::UnboundedSender<Option<ThemeMode>>) {
        tokio::spawn(async move {
            if let Err(err) = Self::run(tx).await {
                error!(error = %err, "XDG Settings Portal observer exited with error");
            }
        });
    }

    async fn run(tx: mpsc::UnboundedSender<Option<ThemeMode>>) -> anyhow::Result<()> {
        let settings = Settings::new().await?;

        if let Ok(initial_scheme) = settings.color_scheme().await {
            let mode = map_color_scheme(initial_scheme);
            info!(initial_portal_appearance = ?mode, "Fetched initial XDG portal color-scheme");
            let _ = tx.send(mode);
        }

        let mut stream = settings.receive_color_scheme_changed().await?;
        debug!("Subscribed to XDG Portal color-scheme change signals");

        use futures_lite::stream::StreamExt;
        while let Some(scheme) = stream.next().await {
            let mode = map_color_scheme(scheme);
            info!(portal_appearance = ?mode, "Received XDG portal color-scheme change signal");
            if tx.send(mode).is_err() {
                break;
            }
        }

        Ok(())
    }
}

fn map_color_scheme(scheme: ColorScheme) -> Option<ThemeMode> {
    match scheme {
        ColorScheme::PreferDark => Some(ThemeMode::Dark),
        ColorScheme::PreferLight => Some(ThemeMode::Light),
        ColorScheme::NoPreference => None,
    }
}
