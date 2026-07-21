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
#[derive(Clone, Debug, PartialEq)]
pub struct BatteryInfo {
    pub percentage: u8,
    pub is_charging: bool,
    pub is_present: bool,
}

impl Default for BatteryInfo {
    fn default() -> Self {
        Self {
            percentage: 85,
            is_charging: false,
            is_present: true,
        }
    }
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
                    let pct = proxy.percentage().await.unwrap_or(85.0) as u8;
                    let st = proxy.state().await.unwrap_or(2);
                    let pres = proxy.is_present().await.unwrap_or(true);
                    {
                        let mut lock = info_clone.lock().unwrap();
                        *lock = BatteryInfo {
                            percentage: pct,
                            is_charging: st == 1,
                            is_present: pres,
                        };
                    }
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
