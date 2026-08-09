use anyhow::Result;
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

use futures_lite::StreamExt;
use std::time::Duration;
use tokio::sync::watch;

use crate::runtime::{StateContext, StateRuntime};

const UPOWER_SERVICE: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

async fn fetch_battery_snapshot(proxy: &UPowerDisplayDeviceProxy<'_>) -> BatteryInfo {
    match (
        proxy.percentage().await,
        proxy.state().await,
        proxy.is_present().await,
    ) {
        (Ok(percentage), Ok(state), Ok(is_present)) => BatteryInfo {
            percentage: percentage.clamp(0.0, 100.0) as u8,
            is_charging: state == 1,
            is_present,
        },
        _ => BatteryInfo::default(),
    }
}

async fn run_upower_loop(ctx: StateContext<BatteryInfo>) {
    loop {
        let connection = match Connection::system().await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::debug!("UPower D-Bus system connection failed: {err}; retrying");
                ctx.send_replace(BatteryInfo::default());
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            }
        };

        let proxy = match UPowerDisplayDeviceProxy::new(&connection).await {
            Ok(proxy) => proxy,
            Err(err) => {
                tracing::debug!("UPower DisplayDevice proxy failed: {err}; retrying");
                ctx.send_replace(BatteryInfo::default());
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            }
        };

        let properties = match zbus::fdo::PropertiesProxy::builder(&connection)
            .destination(UPOWER_SERVICE)
            .and_then(|b| b.path(DISPLAY_DEVICE_PATH))
        {
            Ok(builder) => match builder.build().await {
                Ok(props) => props,
                Err(err) => {
                    tracing::debug!("UPower PropertiesProxy build failed: {err}; retrying");
                    ctx.send_replace(BatteryInfo::default());
                    tokio::time::sleep(RECONNECT_DELAY).await;
                    continue;
                }
            },
            Err(err) => {
                tracing::debug!("UPower PropertiesProxy setup failed: {err}; retrying");
                ctx.send_replace(BatteryInfo::default());
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            }
        };

        let mut changes = match properties.receive_properties_changed().await {
            Ok(stream) => stream,
            Err(err) => {
                tracing::debug!("UPower receive_properties_changed failed: {err}; retrying");
                ctx.send_replace(BatteryInfo::default());
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            }
        };

        // One initial DisplayDevice snapshot
        ctx.send_replace(fetch_battery_snapshot(&proxy).await);

        let owner_changes = if let Ok(dbus) = zbus::fdo::DBusProxy::new(&connection).await {
            dbus.receive_name_owner_changed().await.ok()
        } else {
            None
        };

        if let Some(mut owner_changes) = owner_changes {
            loop {
                tokio::select! {
                    change = changes.next() => {
                        let Some(change) = change else { break; };
                        if let Ok(args) = change.args()
                            && args.interface_name == "org.freedesktop.UPower.Device"
                        {
                            ctx.send_replace(fetch_battery_snapshot(&proxy).await);
                        }
                    }
                    owner = owner_changes.next() => {
                        if let Some(owner) = owner
                            && owner.args().ok().is_some_and(|args| args.name.as_str() == UPOWER_SERVICE)
                        {
                            break;
                        }
                    }
                }
            }
        } else {
            while let Some(change) = changes.next().await {
                if let Ok(args) = change.args()
                    && args.interface_name == "org.freedesktop.UPower.Device"
                {
                    ctx.send_replace(fetch_battery_snapshot(&proxy).await);
                }
            }
        }

        // Connection lost or stream terminated
        ctx.send_replace(BatteryInfo::default());
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

/// UPower battery service for tracking battery percentage and charging state event-driven.
#[derive(Clone)]
pub struct BatteryService {
    runtime: StateRuntime<BatteryInfo>,
}

impl Default for BatteryService {
    fn default() -> Self {
        Self::new_offline()
    }
}

impl BatteryService {
    pub fn new() -> Result<Self> {
        let runtime = StateRuntime::spawn(
            BatteryInfo::default(),
            BatteryInfo::default(),
            run_upower_loop,
        );
        Ok(Self { runtime })
    }

    pub fn new_offline() -> Self {
        let runtime = StateRuntime::new_offline(BatteryInfo::default());
        Self { runtime }
    }

    pub fn subscribe(&self) -> watch::Receiver<BatteryInfo> {
        self.runtime.subscribe()
    }

    pub fn battery_info(&self) -> BatteryInfo {
        self.runtime.get()
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
