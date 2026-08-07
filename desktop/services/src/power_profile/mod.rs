use anyhow::Result;
use tokio::sync::{mpsc, watch};

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
    pub(crate) fn offline() -> Self {
        Self {
            active_profile: PowerProfile::Balanced,
            available: false,
        }
    }

    /// A live state read from the daemon.
    #[cfg(target_os = "linux")]
    pub(crate) fn online(active_profile: &str) -> Self {
        Self::live(PowerProfile::parse(active_profile))
    }

    /// A live state derived from a parsed profile.
    #[cfg(target_os = "linux")]
    pub(crate) fn live(active_profile: PowerProfile) -> Self {
        Self {
            active_profile,
            available: true,
        }
    }
}

/// Commands dispatched to the backend event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PowerProfileCommand {
    Set(PowerProfile),
}

#[cfg(target_os = "linux")]
mod adapter;

#[cfg(target_os = "linux")]
use adapter::{PowerProfileAdapter, ZbusPowerProfileAdapter};

/// Event-driven service for the `net.hadess.PowerProfiles` daemon.
///
/// A backend adapter owns an async loop that watches the daemon for
/// `PropertiesChanged` signals and forwards state updates through a
/// `watch::Sender`, replacing the former CLI polling approach.
pub struct PowerProfileService {
    tx: watch::Sender<PowerProfileInfo>,
    command_tx: Option<mpsc::UnboundedSender<PowerProfileCommand>>,
    _task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for PowerProfileService {
    fn drop(&mut self) {
        if let Some(task) = self._task.take() {
            task.abort();
        }
    }
}

impl PowerProfileService {
    pub fn new() -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            Ok(Self::from_adapter(ZbusPowerProfileAdapter))
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self::new_offline())
        }
    }

    /// Spawns the adapter's event loop and wires it to the service's state.
    #[cfg(target_os = "linux")]
    fn from_adapter(adapter: impl PowerProfileAdapter) -> Self {
        let (tx, _) = watch::channel(PowerProfileInfo::offline());
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let task = adapter.spawn(tx.clone(), command_rx);
        Self {
            tx,
            command_tx: Some(command_tx),
            _task: Some(task),
        }
    }

    pub fn new_offline() -> Self {
        let (tx, _) = watch::channel(PowerProfileInfo::offline());
        Self {
            tx,
            command_tx: None,
            _task: None,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<PowerProfileInfo> {
        self.tx.subscribe()
    }

    pub fn info(&self) -> PowerProfileInfo {
        self.tx.borrow().clone()
    }

    /// Dispatches a profile change to the daemon and optimistically forwards
    /// the requested state to subscribers. The daemon's own
    /// `PropertiesChanged` signal confirms (or corrects) the state.
    pub fn set_profile(&self, profile: PowerProfile) -> bool {
        let Some(command_tx) = &self.command_tx else {
            return false;
        };
        if !self.tx.borrow().available {
            return false;
        }
        if command_tx
            .send(PowerProfileCommand::Set(profile.clone()))
            .is_ok()
        {
            let mut info = self.tx.borrow().clone();
            info.active_profile = profile;
            let _ = self.tx.send_replace(info);
            true
        } else {
            false
        }
    }

    /// Constructs a service backed by the given adapter. Test-only.
    #[cfg(all(test, target_os = "linux"))]
    fn with_adapter(adapter: impl PowerProfileAdapter) -> Self {
        Self::from_adapter(adapter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use crate::power_profile::adapter::mock::TestPowerProfileAdapter;
    use std::time::Duration;

    #[test]
    fn test_power_profile_offline() {
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

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_adapter_forwards_simulated_property_changes() {
        let (adapter, events) = TestPowerProfileAdapter::new();
        let service = PowerProfileService::with_adapter(adapter);
        let mut receiver = service.subscribe();

        assert_eq!(receiver.borrow().active_profile, PowerProfile::Balanced);
        assert!(!receiver.borrow().available);

        events
            .send(PowerProfileInfo {
                active_profile: PowerProfile::Performance,
                available: true,
            })
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), receiver.changed())
                .await
                .is_ok()
        );
        assert_eq!(receiver.borrow().active_profile, PowerProfile::Performance);
        assert!(receiver.borrow().available);

        events
            .send(PowerProfileInfo {
                active_profile: PowerProfile::PowerSaver,
                available: true,
            })
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
        let service = PowerProfileService::with_adapter(adapter);
        let mut receiver = service.subscribe();

        assert!(!receiver.borrow().available);
        assert!(!service.set_profile(PowerProfile::Performance));

        events.send(PowerProfileInfo::online("balanced")).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), receiver.changed())
                .await
                .is_ok()
        );
        assert!(receiver.borrow().available);
        assert_eq!(receiver.borrow().active_profile, PowerProfile::Balanced);

        assert!(service.set_profile(PowerProfile::PowerSaver));
        assert!(
            tokio::time::timeout(Duration::from_secs(1), receiver.changed())
                .await
                .is_ok()
        );
        assert_eq!(receiver.borrow().active_profile, PowerProfile::PowerSaver);
        assert!(receiver.borrow().available);
    }
}
