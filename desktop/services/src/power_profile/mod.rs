use anyhow::Result;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}

impl PowerProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "power-saver" => Self::PowerSaver,
            "performance" => Self::Performance,
            _ => Self::Balanced,
        }
    }
}

use crate::polled::PolledService;
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerProfileInfo {
    pub active_profile: PowerProfile,
    pub available: bool,
}

pub struct PowerProfileService {
    polled: PolledService<PowerProfileInfo>,
}

impl PowerProfileService {
    pub fn new() -> Result<Self> {
        let (available, active_profile) = Self::query_system();
        let initial = PowerProfileInfo {
            active_profile,
            available,
        };
        let polled = PolledService::new(
            initial,
            Duration::from_secs(3),
            None,
            |_curr| -> Result<PowerProfileInfo, std::convert::Infallible> {
                let (available, active_profile) = Self::query_system();
                Ok(PowerProfileInfo {
                    active_profile,
                    available,
                })
            },
        );
        Ok(Self { polled })
    }

    pub fn new_offline() -> Self {
        Self {
            polled: PolledService::new_offline(PowerProfileInfo {
                active_profile: PowerProfile::Balanced,
                available: false,
            }),
        }
    }

    fn query_system() -> (bool, PowerProfile) {
        if let Ok(output) = Command::new("powerprofilesctl").arg("get").output()
            && output.status.success()
        {
            let s = String::from_utf8_lossy(&output.stdout);
            (true, PowerProfile::parse(&s))
        } else {
            (false, PowerProfile::Balanced)
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<PowerProfileInfo> {
        self.polled.subscribe()
    }

    pub fn info(&self) -> PowerProfileInfo {
        self.polled.get()
    }

    pub fn set_profile(&self, profile: PowerProfile) -> bool {
        let mut current = self.polled.get();
        if !current.available {
            return false;
        }

        if Command::new("powerprofilesctl")
            .args(["set", profile.as_str()])
            .status()
            .is_ok_and(|s| s.success())
        {
            current.active_profile = profile;
            self.polled.send_replace(current);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_profile_offline() {
        let service = PowerProfileService::new_offline();
        let info = service.info();
        assert!(!info.available);
        assert_eq!(info.active_profile, PowerProfile::Balanced);
        assert!(!service.set_profile(PowerProfile::PowerSaver));
    }
}
