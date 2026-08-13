pub mod color;
pub mod dbus;
pub mod wayland;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

/// Represents status of the system night light / color temperature service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NightLightInfo {
    pub is_active: bool,
    pub temperature_kelvin: u32,
    pub available: bool,
    pub backend_name: String,
}

impl Default for NightLightInfo {
    fn default() -> Self {
        Self {
            is_active: false,
            temperature_kelvin: 6500,
            available: false,
            backend_name: "none".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ThemeSchedule {
    Manual,
    #[default]
    SunsetToSunrise,
    Scheduled {
        start_hour: u8,
        end_hour: u8,
    },
}

pub fn should_use_dark_mode(schedule: &ThemeSchedule, current_hour: u8) -> bool {
    match schedule {
        ThemeSchedule::Manual => true,
        ThemeSchedule::SunsetToSunrise => !(6..18).contains(&current_hour),
        ThemeSchedule::Scheduled {
            start_hour,
            end_hour,
        } => {
            if start_hour < end_hour {
                current_hour >= *start_hour && current_hour < *end_hour
            } else {
                current_hour >= *start_hour || current_hour < *end_hour
            }
        }
    }
}

#[derive(Debug)]
enum NightLightCommand {
    SetActive(bool),
    SetTemperature(u32),
}

enum ActiveBackend {
    WlrWayland(wayland::WlrGammaBackend),
    Gnome(dbus::GnomeColorBackend<'static>),
    Kde(dbus::KdeNightLightBackend<'static>),
}

/// Service managing Wayland night light color temperature via native WLR gamma control or DBus fallback.
pub struct NightLightService {
    tx: watch::Sender<NightLightInfo>,
    cmd_tx: Option<mpsc::UnboundedSender<NightLightCommand>>,
    _task: Option<Arc<tokio::task::JoinHandle<()>>>,
}

impl NightLightService {
    pub fn new() -> Result<Self> {
        let (tx, _) = watch::channel(NightLightInfo::default());
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<NightLightCommand>();

        let tx_clone = tx.clone();
        let task = tokio::spawn(async move {
            run_backend_loop(tx_clone, &mut cmd_rx).await;
        });

        Ok(Self {
            tx,
            cmd_tx: Some(cmd_tx),
            _task: Some(Arc::new(task)),
        })
    }

    pub fn new_offline() -> Self {
        let (tx, _) = watch::channel(NightLightInfo::default());
        Self {
            tx,
            cmd_tx: None,
            _task: None,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<NightLightInfo> {
        self.tx.subscribe()
    }

    pub fn info(&self) -> NightLightInfo {
        self.tx.borrow().clone()
    }

    pub fn set_active(&self, active: bool) -> bool {
        let current = self.info();
        if !current.available {
            return false;
        }

        if let Some(ref cmd_tx) = self.cmd_tx {
            let _ = cmd_tx.send(NightLightCommand::SetActive(active));
        }

        let mut updated = current;
        updated.is_active = active;
        if active && updated.temperature_kelvin == 6500 {
            updated.temperature_kelvin = 3500;
        } else if !active {
            updated.temperature_kelvin = 6500;
        }
        self.tx.send_replace(updated);
        active
    }

    pub fn set_temperature(&self, kelvin: u32) -> bool {
        let current = self.info();
        if !current.available {
            return false;
        }

        if let Some(ref cmd_tx) = self.cmd_tx {
            let _ = cmd_tx.send(NightLightCommand::SetTemperature(kelvin));
        }

        let mut updated = current;
        updated.temperature_kelvin = kelvin;
        self.tx.send_replace(updated);
        true
    }

    pub fn toggle(&self) -> bool {
        let active = self.info().is_active;
        self.set_active(!active)
    }
}

async fn run_backend_loop(
    tx: watch::Sender<NightLightInfo>,
    cmd_rx: &mut mpsc::UnboundedReceiver<NightLightCommand>,
) {
    let active_backend = if let Ok(backend) = wayland::WlrGammaBackend::try_init() {
        info!("NightLightService using native WLR Wayland gamma control");
        Some((
            ActiveBackend::WlrWayland(backend),
            "wlr-gamma-control".to_string(),
        ))
    } else if let Ok(conn) = zbus::Connection::session().await {
        if let Ok(gnome) = dbus::GnomeColorBackend::try_init(&conn).await {
            info!("NightLightService using GNOME DBus color service");
            Some((ActiveBackend::Gnome(gnome), "gnome".to_string()))
        } else if let Ok(kde) = dbus::KdeNightLightBackend::try_init(&conn).await {
            info!("NightLightService using KDE DBus night light service");
            Some((ActiveBackend::Kde(kde), "kde".to_string()))
        } else {
            None
        }
    } else {
        None
    };

    let (mut backend, backend_name, available) = match active_backend {
        Some((b, name)) => (Some(b), name, true),
        None => (None, "none".to_string(), false),
    };

    let mut info = NightLightInfo {
        is_active: false,
        temperature_kelvin: 6500,
        available,
        backend_name,
    };
    tx.send_replace(info.clone());

    if !available {
        return;
    }

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            NightLightCommand::SetActive(active) => {
                info.is_active = active;
                if active && info.temperature_kelvin == 6500 {
                    info.temperature_kelvin = 3500;
                } else if !active {
                    info.temperature_kelvin = 6500;
                }
            }
            NightLightCommand::SetTemperature(kelvin) => {
                info.temperature_kelvin = kelvin;
            }
        }

        if let Some(ref mut b) = backend {
            let res = match b {
                ActiveBackend::WlrWayland(wlr) => {
                    wlr.apply(info.is_active, info.temperature_kelvin)
                }
                ActiveBackend::Gnome(gnome) => {
                    gnome.apply(info.is_active, info.temperature_kelvin).await
                }
                ActiveBackend::Kde(kde) => kde.apply(info.is_active, info.temperature_kelvin).await,
            };
            if let Err(e) = res {
                warn!("Failed to apply night light settings: {:#}", e);
            }
        }

        tx.send_replace(info.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_night_light_service_offline() {
        let service = NightLightService::new_offline();
        let info = service.info();
        assert!(!info.available);
        assert!(!info.is_active);
        assert_eq!(info.temperature_kelvin, 6500);

        let toggled = service.toggle();
        assert!(!toggled);
    }

    #[test]
    fn test_theme_schedule_policy() {
        assert!(should_use_dark_mode(&ThemeSchedule::Manual, 12));
        assert!(should_use_dark_mode(&ThemeSchedule::SunsetToSunrise, 20));
        assert!(should_use_dark_mode(&ThemeSchedule::SunsetToSunrise, 2));
        assert!(!should_use_dark_mode(&ThemeSchedule::SunsetToSunrise, 12));

        let sched = ThemeSchedule::Scheduled {
            start_hour: 22,
            end_hour: 7,
        };
        assert!(should_use_dark_mode(&sched, 23));
        assert!(should_use_dark_mode(&sched, 5));
        assert!(!should_use_dark_mode(&sched, 14));
    }
}
