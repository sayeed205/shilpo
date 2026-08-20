use ashpd::desktop::settings::{ColorScheme, Settings};
use shilpo_m3e::theme::ThemeMode;
use tokio::sync::watch;
use tracing::{debug, error, info};

pub struct PortalObserver;

impl PortalObserver {
    /// Spawn the portal observer task.
    ///
    /// Returns a `watch::Receiver` that always carries the **latest** observed
    /// portal color-scheme preference. The outer `Option` means "no signal
    /// received yet"; the inner `Option<ThemeMode>` is the actual appearance
    /// (where `None` = `ColorScheme::NoPreference`). Do not collapse these two
    /// levels — they carry distinct semantics.
    ///
    /// `watch` is deliberately used instead of a capacity-1 mpsc with
    /// `try_send`. `portal_tx` carries a value (`Option<ThemeMode>`); keeping
    /// the old item and dropping the new one would retain a **stale**
    /// appearance. `watch` is genuinely replace-latest, so the daemon always
    /// sees the most-recent preference.
    pub async fn start(tx: watch::Sender<Option<Option<ThemeMode>>>) {
        tokio::spawn(async move {
            if let Err(err) = Self::run(tx).await {
                error!(error = %err, "XDG Settings Portal observer exited with error");
            }
        });
    }

    async fn run(tx: watch::Sender<Option<Option<ThemeMode>>>) -> anyhow::Result<()> {
        let settings = Settings::new().await?;

        if let Ok(initial_scheme) = settings.color_scheme().await {
            let mode = map_color_scheme(initial_scheme);
            info!(initial_portal_appearance = ?mode, "Fetched initial XDG portal color-scheme");
            // Ignore error: if the receiver is gone the daemon is shutting down.
            let _ = tx.send(Some(mode));
        }

        let mut stream = settings.receive_color_scheme_changed().await?;
        debug!("Subscribed to XDG Portal color-scheme change signals");

        use futures_lite::stream::StreamExt;
        while let Some(scheme) = stream.next().await {
            let mode = map_color_scheme(scheme);
            info!(portal_appearance = ?mode, "Received XDG portal color-scheme change signal");
            // send_replace never fails on capacity; break only if all receivers dropped.
            if tx.receiver_count() == 0 {
                break;
            }
            tx.send_replace(Some(mode));
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
