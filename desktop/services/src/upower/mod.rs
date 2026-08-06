use anyhow::Result;
use std::sync::{Arc, Mutex};
use zbus::{Connection, proxy};

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
pub trait UPowerDisplayDevice {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
}

/// Battery state information.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatteryInfo {
    pub percentage: u8,
    pub is_charging: bool,
    pub is_present: bool,
}

impl BatteryInfo {
    pub fn is_low_battery(&self) -> bool {
        self.is_present && !self.is_charging && self.percentage < 15
    }

    pub fn low_power_mode(&self) -> bool {
        self.is_present && !self.is_charging && self.percentage < 20
    }
}

/// UPower battery service for tracking battery percentage and charging state.
pub struct BatteryService {
    info: Arc<Mutex<BatteryInfo>>,
    _task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for BatteryService {
    fn drop(&mut self) {
        if let Some(task) = self._task.take() {
            task.abort();
        }
    }
}

impl BatteryService {
    pub fn new() -> Result<Self> {
        let info = Arc::new(Mutex::new(BatteryInfo::default()));

        // Attempt async connection and polling of UPower D-Bus
        let info_clone = info.clone();
        let task = tokio::spawn(async move {
            if let Ok(connection) = Connection::system().await
                && let Ok(proxy) = UPowerDisplayDeviceProxy::new(&connection).await
            {
                loop {
                    let next_info = match (
                        proxy.percentage().await,
                        proxy.state().await,
                        proxy.is_present().await,
                    ) {
                        (Ok(percentage), Ok(state), Ok(is_present)) => Some(BatteryInfo {
                            percentage: percentage.clamp(0.0, 100.0) as u8,
                            is_charging: state == 1,
                            is_present,
                        }),
                        _ => None,
                    };
                    *info_clone.lock().unwrap() = next_info.unwrap_or_default();
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                }
            }
        });

        Ok(Self {
            info,
            _task: Some(task),
        })
    }

    pub fn battery_info(&self) -> BatteryInfo {
        self.info.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unavailable() {
        assert_eq!(BatteryInfo::default().percentage, 0);
        assert!(!BatteryInfo::default().is_charging);
        assert!(!BatteryInfo::default().is_present);
    }

    #[test]
    fn test_battery_low_power_threshold_policy() {
        let mut info = BatteryInfo {
            percentage: 10,
            is_charging: false,
            is_present: true,
        };
        assert!(info.is_low_battery());
        assert!(info.low_power_mode());

        info.is_charging = true;
        assert!(!info.is_low_battery());
        assert!(!info.low_power_mode());
    }
}
