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

/// UPower battery service for tracking battery percentage and charging state.
pub struct BatteryService {
    info: Arc<Mutex<BatteryInfo>>,
}

impl BatteryService {
    pub fn new() -> Result<Self> {
        let info = Arc::new(Mutex::new(BatteryInfo::default()));
        let service = Self { info };

        // Attempt async connection and polling of UPower D-Bus
        let info_clone = service.info.clone();
        tokio::spawn(async move {
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

        Ok(service)
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
}
