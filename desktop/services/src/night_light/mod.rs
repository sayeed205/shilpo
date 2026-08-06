use anyhow::Result;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// Represents status of the system night light / color temperature service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NightLightInfo {
    pub is_active: bool,
    pub temperature_kelvin: u32,
    pub available: bool,
    pub backend_name: String,
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

use crate::polled::PolledService;
use std::time::Duration;
use tokio::sync::watch;

/// Service managing Wayland night light color temperature via sunsetr / wlsunset / gammastep.
pub struct NightLightService {
    polled: PolledService<NightLightInfo>,
    _child_process: Arc<Mutex<Option<std::process::Child>>>,
}

impl NightLightService {
    pub fn new() -> Result<Self> {
        let (available, backend_name) = Self::detect_backend();
        let initial = NightLightInfo {
            is_active: false,
            temperature_kelvin: 6500,
            available,
            backend_name,
        };
        let polled = PolledService::new(
            initial,
            Duration::from_secs(3),
            None,
            |current: &NightLightInfo| -> Result<NightLightInfo, std::convert::Infallible> {
                let (available, backend_name) = Self::detect_backend();
                let mut updated = current.clone();
                updated.available = available;
                updated.backend_name = backend_name;
                Ok(updated)
            },
        );

        Ok(Self {
            polled,
            _child_process: Arc::new(Mutex::new(None)),
        })
    }

    pub fn new_offline() -> Self {
        Self {
            polled: PolledService::new_offline(NightLightInfo::default()),
            _child_process: Arc::new(Mutex::new(None)),
        }
    }

    fn detect_backend() -> (bool, String) {
        if Command::new("sunsetr").arg("--version").output().is_ok() {
            (true, "sunsetr".into())
        } else if Command::new("wlsunset").arg("-h").output().is_ok() {
            (true, "wlsunset".into())
        } else if Command::new("gammastep").arg("-h").output().is_ok() {
            (true, "gammastep".into())
        } else {
            (false, "none".into())
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<NightLightInfo> {
        self.polled.subscribe()
    }

    pub fn info(&self) -> NightLightInfo {
        self.polled.get()
    }

    pub fn set_active(&self, active: bool) -> bool {
        let mut info_state = self.polled.get();
        let mut process_lock = self._child_process.lock().unwrap();

        if !info_state.available {
            return false;
        }

        if active {
            match info_state.backend_name.as_str() {
                "sunsetr" => {
                    let _ = Command::new("sunsetr").args(["set", "3500"]).spawn();
                }
                "wlsunset" => {
                    if let Some(mut child) = process_lock.take() {
                        let _ = child.kill();
                    }
                    if let Ok(child) = Command::new("wlsunset")
                        .args(["-t", "3500", "-T", "6500"])
                        .spawn()
                    {
                        *process_lock = Some(child);
                    }
                }
                "gammastep" => {
                    let _ = Command::new("gammastep").args(["-O", "3500"]).spawn();
                }
                _ => {}
            }
            info_state.is_active = true;
            info_state.temperature_kelvin = 3500;
        } else {
            match info_state.backend_name.as_str() {
                "sunsetr" => {
                    let _ = Command::new("sunsetr").args(["set", "6500"]).spawn();
                }
                "wlsunset" => {
                    if let Some(mut child) = process_lock.take() {
                        let _ = child.kill();
                    }
                }
                "gammastep" => {
                    let _ = Command::new("gammastep").arg("-x").spawn();
                }
                _ => {}
            }
            info_state.is_active = false;
            info_state.temperature_kelvin = 6500;
        }
        self.polled.send_replace(info_state);
        true
    }

    pub fn toggle(&self) -> bool {
        let active = self.polled.get().is_active;
        self.set_active(!active)
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
