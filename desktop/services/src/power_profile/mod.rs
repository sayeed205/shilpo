use tokio::sync::watch;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerProfileInfo {
    pub active_profile: PowerProfile,
    pub available: bool,
}

impl PowerProfileInfo {
    /// The fallback state used when the daemon is unreachable.
    pub(crate) fn fallback() -> Self {
        Self {
            active_profile: PowerProfile::Balanced,
            available: false,
        }
    }

    /// A live state read from the daemon's `ActiveProfile` property.
    #[cfg(target_os = "linux")]
    pub(crate) fn from_daemon(active_profile: &str) -> Self {
        Self::online(PowerProfile::parse(active_profile))
    }

    /// A live state derived from a parsed profile.
    #[cfg(target_os = "linux")]
    pub(crate) fn online(active_profile: PowerProfile) -> Self {
        Self {
            active_profile,
            available: true,
        }
    }
}

/// Commands dispatched to the backend event loop.
#[derive(Debug)]
enum PowerProfileCommand {
    Set(PowerProfile),
}

use crate::runtime::CommandRuntime;

#[cfg(target_os = "linux")]
mod adapter;

#[cfg(target_os = "linux")]
use adapter::{PowerProfileAdapter, ZbusPowerProfileAdapter};

/// Event-driven service for the `net.hadess.PowerProfiles` daemon.
///
/// A backend adapter owns an async loop that watches the daemon for
/// `PropertiesChanged` signals and forwards state updates through a
/// `watch::Sender`, replacing the former CLI polling approach.
///
/// Clones share the same daemon connection, command channel, and state.
#[derive(Clone)]
pub struct PowerProfileService {
    runtime: CommandRuntime<PowerProfileInfo, PowerProfileCommand>,
}

impl Default for PowerProfileService {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerProfileService {
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::from_adapter(ZbusPowerProfileAdapter)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self::new_offline()
        }
    }

    /// Spawns the adapter's event loop and wires it to the service's state.
    #[cfg(target_os = "linux")]
    fn from_adapter(adapter: impl PowerProfileAdapter) -> Self {
        Self {
            runtime: CommandRuntime::spawn(PowerProfileInfo::fallback(), move |ctx| {
                adapter.run(ctx)
            }),
        }
    }

    pub fn new_offline() -> Self {
        Self {
            runtime: CommandRuntime::new_offline(PowerProfileInfo::fallback()),
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<PowerProfileInfo> {
        self.runtime.subscribe()
    }

    pub fn info(&self) -> PowerProfileInfo {
        self.runtime.get()
    }

    /// Dispatches a profile change command to the backend adapter.
    ///
    /// Returns `true` if the service is online and the command was sent.
    /// Subscribers observe state changes through the watch channel when
    /// the daemon confirms the update.
    pub fn set_profile(&self, profile: PowerProfile) -> bool {
        if !self.runtime.get().available {
            return false;
        }
        self.runtime.send_command(PowerProfileCommand::Set(profile))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use crate::power_profile::adapter::mock::TestPowerProfileAdapter;
    use std::time::Duration;

    #[tokio::test]
    async fn test_power_profile_offline() {
        let service = PowerProfileService::new_offline();
        let info = service.info();
        assert!(!info.available);
        assert_eq!(info.active_profile, PowerProfile::Balanced);
        assert!(!service.set_profile(PowerProfile::PowerSaver));
    }

    #[test]
    fn test_parse_power_profiles() {
        assert_eq!(PowerProfile::parse("power-saver"), PowerProfile::PowerSaver);
        assert_eq!(
            PowerProfile::parse("performance"),
            PowerProfile::Performance
        );
        assert_eq!(PowerProfile::parse("balanced"), PowerProfile::Balanced);
        assert_eq!(PowerProfile::parse("unknown"), PowerProfile::Balanced);
    }

    #[test]
    fn test_power_profile_info_states() {
        let fallback = PowerProfileInfo::fallback();
        assert!(!fallback.available);
        assert_eq!(fallback.active_profile, PowerProfile::Balanced);

        #[cfg(target_os = "linux")]
        {
            let daemon = PowerProfileInfo::from_daemon("power-saver");
            assert!(daemon.available);
            assert_eq!(daemon.active_profile, PowerProfile::PowerSaver);

            let online = PowerProfileInfo::online(PowerProfile::Performance);
            assert!(online.available);
            assert_eq!(online.active_profile, PowerProfile::Performance);
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_adapter_forwards_simulated_property_changes() {
        let (adapter, events) = TestPowerProfileAdapter::new();
        let service = PowerProfileService::from_adapter(adapter);
        let mut receiver = service.subscribe();

        assert_eq!(receiver.borrow().active_profile, PowerProfile::Balanced);
        assert!(!receiver.borrow().available);

        events
            .send(PowerProfileInfo::online(PowerProfile::Performance))
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), receiver.changed())
                .await
                .is_ok()
        );
        assert_eq!(receiver.borrow().active_profile, PowerProfile::Performance);
        assert!(receiver.borrow().available);

        events
            .send(PowerProfileInfo::online(PowerProfile::PowerSaver))
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), receiver.changed())
                .await
                .is_ok()
        );
        assert_eq!(receiver.borrow().active_profile, PowerProfile::PowerSaver);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_adapter_applies_set_profile_command() {
        let (adapter, events) = TestPowerProfileAdapter::new();
        let service = PowerProfileService::from_adapter(adapter);
        let mut receiver = service.subscribe();

        assert!(!receiver.borrow().available);
        assert!(!service.set_profile(PowerProfile::PowerSaver));

        events
            .send(PowerProfileInfo::from_daemon("balanced"))
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), receiver.changed())
                .await
                .is_ok()
        );
        assert_eq!(receiver.borrow().active_profile, PowerProfile::Balanced);
        assert!(receiver.borrow().available);

        assert!(service.set_profile(PowerProfile::PowerSaver));
        assert!(
            tokio::time::timeout(Duration::from_secs(1), receiver.changed())
                .await
                .is_ok()
        );
        assert_eq!(receiver.borrow().active_profile, PowerProfile::PowerSaver);
    }
}
