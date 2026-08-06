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

/// Service managing Wayland night light color temperature via sunsetr / wlsunset / gammastep.
pub struct NightLightService {
    info: Arc<Mutex<NightLightInfo>>,
    _child_process: Arc<Mutex<Option<std::process::Child>>>,
}

impl NightLightService {
    pub fn new() -> Result<Self> {
        let (available, backend_name) = Self::detect_backend();
        let info = Arc::new(Mutex::new(NightLightInfo {
            is_active: false,
            temperature_kelvin: 6500,
            available,
            backend_name,
        }));

        Ok(Self {
            info,
            _child_process: Arc::new(Mutex::new(None)),
        })
    }

    pub fn new_offline() -> Self {
        Self {
            info: Arc::new(Mutex::new(NightLightInfo::default())),
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

    pub fn info(&self) -> NightLightInfo {
        self.info.lock().unwrap().clone()
    }

    pub fn set_active(&self, active: bool) -> bool {
        let mut info_lock = self.info.lock().unwrap();
        let mut process_lock = self._child_process.lock().unwrap();

        if !info_lock.available {
            return false;
        }

        if active {
            match info_lock.backend_name.as_str() {
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
            info_lock.is_active = true;
            info_lock.temperature_kelvin = 3500;
        } else {
            match info_lock.backend_name.as_str() {
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
            info_lock.is_active = false;
            info_lock.temperature_kelvin = 6500;
        }
        true
    }

    pub fn toggle(&self) -> bool {
        let active = self.info.lock().unwrap().is_active;
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
