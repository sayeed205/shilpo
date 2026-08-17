use std::time::Duration;

use anyhow::Result;
use futures_lite::StreamExt;
use tokio::sync::watch;
use zbus::{Connection, proxy};

pub use crate::device_protocol::{
    BatteryChargeState, BatteryCoarseLevel, BatteryDevicePayload, BatteryPayload as BatteryInfo,
    BatteryTechnology, BatteryWarningLevel, OptionalBool, OptionalF64, OptionalU64,
};
use crate::runtime::{StateContext, StateRuntime};

const UPOWER_SERVICE: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower"
)]
pub trait UPowerDevice {
    #[zbus(property, name = "Type")]
    fn type_(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn technology(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn power_supply(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;

    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;

    #[zbus(property)]
    fn energy_rate(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn capacity(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn energy(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn energy_empty(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn energy_full(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn energy_full_design(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn voltage(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn voltage_min_design(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn voltage_max_design(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn temperature(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    #[zbus(property, name = "ChargeCycles")]
    fn charge_cycles(&self) -> zbus::Result<i32>;

    #[zbus(property)]
    fn update_time(&self) -> zbus::Result<u64>;

    #[zbus(property)]
    fn is_rechargeable(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn warning_level(&self) -> zbus::Result<u32>;

    #[zbus(property, name = "BatteryLevel")]
    fn coarse_level(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn has_history(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn has_statistics(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn vendor(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn serial(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn native_path(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn charge_start_threshold(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn charge_end_threshold(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn charge_threshold_supported(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn charge_threshold_enabled(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn charge_threshold_settings_supported(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
pub trait UPowerManager {
    fn enumerate_devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    #[zbus(signal)]
    fn device_added(&self, device: zbus::zvariant::OwnedObjectPath) -> zbus::Result<()>;

    #[zbus(signal)]
    fn device_removed(&self, device: zbus::zvariant::OwnedObjectPath) -> zbus::Result<()>;
}

async fn fetch_physical_device_snapshot(
    connection: &Connection,
    path: &zbus::zvariant::ObjectPath<'_>,
) -> Option<BatteryDevicePayload> {
    let proxy = UPowerDeviceProxy::builder(connection)
        .path(path)
        .ok()?
        .build()
        .await
        .ok()?;

    let dev_type = proxy.type_().await.unwrap_or(0);
    let power_supply = proxy.power_supply().await.unwrap_or(false);
    let is_present = proxy.is_present().await.unwrap_or(true);
    if !is_system_battery(dev_type, power_supply, is_present) {
        return None;
    }

    Some(BatteryDevicePayload {
        id: path.as_str().to_owned(),
        vendor: proxy.vendor().await.unwrap_or_default().trim().to_owned(),
        model: proxy.model().await.unwrap_or_default().trim().to_owned(),
        serial: proxy.serial().await.unwrap_or_default().trim().to_owned(),
        native_path: proxy
            .native_path()
            .await
            .unwrap_or_default()
            .trim()
            .to_owned(),
        is_present,
        power_supply,
        percentage: optional_percent(proxy.percentage().await),
        technology: BatteryTechnology::from_u32(proxy.technology().await.unwrap_or(0)),
        charge_state: BatteryChargeState::from_u32(proxy.state().await.unwrap_or(0)),
        time_to_empty_secs: optional_positive_i64(proxy.time_to_empty().await),
        time_to_full_secs: optional_positive_i64(proxy.time_to_full().await),
        energy_wh: optional_positive_f64(proxy.energy().await),
        energy_empty_wh: optional_nonnegative_f64(proxy.energy_empty().await),
        energy_full_wh: optional_positive_f64(proxy.energy_full().await),
        energy_full_design_wh: optional_positive_f64(proxy.energy_full_design().await),
        energy_rate_w: optional_positive_f64(proxy.energy_rate().await),
        capacity_percent: optional_percent(proxy.capacity().await),
        voltage_v: optional_positive_f64(proxy.voltage().await),
        voltage_min_design_v: optional_positive_f64(proxy.voltage_min_design().await),
        voltage_max_design_v: optional_positive_f64(proxy.voltage_max_design().await),
        temperature_c: optional_positive_f64(proxy.temperature().await),
        cycle_count: match proxy.charge_cycles().await {
            Ok(value) if value >= 0 => OptionalU64::some(value as u64),
            _ => OptionalU64::none(),
        },
        update_time: optional_positive_u64(proxy.update_time().await),
        is_rechargeable: proxy
            .is_rechargeable()
            .await
            .map(OptionalBool::some)
            .unwrap_or_else(|_| OptionalBool::none()),
        warning_level: BatteryWarningLevel::from_u32(proxy.warning_level().await.unwrap_or(0)),
        coarse_level: BatteryCoarseLevel::from_u32(proxy.coarse_level().await.unwrap_or(0)),
        has_history: proxy.has_history().await.unwrap_or(false),
        has_statistics: proxy.has_statistics().await.unwrap_or(false),
        charge_start_threshold: optional_threshold(proxy.charge_start_threshold().await),
        charge_end_threshold: optional_threshold(proxy.charge_end_threshold().await),
        charge_threshold_supported: proxy
            .charge_threshold_supported()
            .await
            .map(OptionalBool::some)
            .unwrap_or_else(|_| OptionalBool::none()),
        charge_threshold_enabled: proxy
            .charge_threshold_enabled()
            .await
            .map(OptionalBool::some)
            .unwrap_or_else(|_| OptionalBool::none()),
        charge_threshold_settings_supported: proxy
            .charge_threshold_settings_supported()
            .await
            .map(|value| OptionalU64::some(u64::from(value)))
            .unwrap_or_else(|_| OptionalU64::none()),
    })
}

const fn is_system_battery(device_type: u32, power_supply: bool, is_present: bool) -> bool {
    device_type == 2 && power_supply && is_present
}

fn physical_capacity_percent(devices: &[BatteryDevicePayload]) -> OptionalF64 {
    let (energy_full, energy_design) = devices
        .iter()
        .filter_map(|device| {
            Some((
                device.energy_full_wh.get()?,
                device.energy_full_design_wh.get()?,
            ))
        })
        .fold(
            (0.0, 0.0),
            |(full, design), (device_full, device_design)| {
                (full + device_full, design + device_design)
            },
        );
    if energy_design > 0.0 {
        return OptionalF64::some((energy_full / energy_design * 100.0).clamp(0.0, 100.0));
    }

    let capacities = devices
        .iter()
        .filter_map(|device| device.capacity_percent.get())
        .collect::<Vec<_>>();
    if capacities.is_empty() {
        OptionalF64::none()
    } else {
        OptionalF64::some(capacities.iter().sum::<f64>() / capacities.len() as f64)
    }
}

fn optional_positive_f64(value: zbus::Result<f64>) -> OptionalF64 {
    match value {
        Ok(value) if value.is_finite() && value > 0.0 => OptionalF64::some(value),
        _ => OptionalF64::none(),
    }
}

fn optional_nonnegative_f64(value: zbus::Result<f64>) -> OptionalF64 {
    match value {
        Ok(value) if value.is_finite() && value >= 0.0 => OptionalF64::some(value),
        _ => OptionalF64::none(),
    }
}

fn optional_percent(value: zbus::Result<f64>) -> OptionalF64 {
    match value {
        Ok(value) if value.is_finite() && (0.0..=100.0).contains(&value) => {
            OptionalF64::some(value)
        }
        _ => OptionalF64::none(),
    }
}

fn optional_positive_i64(value: zbus::Result<i64>) -> OptionalU64 {
    match value {
        Ok(value) if value > 0 => OptionalU64::some(value as u64),
        _ => OptionalU64::none(),
    }
}

fn optional_positive_u64(value: zbus::Result<u64>) -> OptionalU64 {
    match value {
        Ok(value) if value > 0 => OptionalU64::some(value),
        _ => OptionalU64::none(),
    }
}

fn optional_threshold(value: zbus::Result<u32>) -> OptionalU64 {
    match value {
        Ok(value) if value <= 100 => OptionalU64::some(u64::from(value)),
        _ => OptionalU64::none(),
    }
}

async fn physical_device_paths(
    manager: &UPowerManagerProxy<'_>,
) -> Vec<zbus::zvariant::OwnedObjectPath> {
    manager.enumerate_devices().await.unwrap_or_default()
}

fn spawn_physical_property_watchers(
    connection: &Connection,
    paths: Vec<zbus::zvariant::OwnedObjectPath>,
    changed: tokio::sync::mpsc::UnboundedSender<()>,
) -> Vec<tokio::task::JoinHandle<()>> {
    paths
        .into_iter()
        .map(|path| {
            let connection = connection.clone();
            let changed = changed.clone();
            tokio::spawn(async move {
                let Ok(builder) = zbus::fdo::PropertiesProxy::builder(&connection)
                    .destination(UPOWER_SERVICE)
                    .and_then(|builder| builder.path(path))
                else {
                    return;
                };
                let Ok(properties) = builder.build().await else {
                    return;
                };
                let Ok(mut stream) = properties.receive_properties_changed().await else {
                    return;
                };
                while let Some(signal) = stream.next().await {
                    if signal
                        .args()
                        .ok()
                        .is_some_and(|args| args.interface_name == "org.freedesktop.UPower.Device")
                    {
                        let _ = changed.send(());
                    }
                }
            })
        })
        .collect()
}

async fn fetch_battery_snapshot(connection: &Connection) -> BatteryInfo {
    let display_proxy = UPowerDeviceProxy::builder(connection)
        .path(DISPLAY_DEVICE_PATH)
        .ok();

    let mut percentage = 0u8;
    let mut available = false;
    let mut state = BatteryChargeState::Unknown;
    let mut is_present = false;
    let mut time_to_empty_secs = OptionalU64::none();
    let mut time_to_full_secs = OptionalU64::none();
    let mut energy_wh = OptionalF64::none();
    let mut energy_empty_wh = OptionalF64::none();
    let mut energy_full_wh = OptionalF64::none();
    let mut energy_full_design_wh = OptionalF64::none();
    let mut energy_rate_w = OptionalF64::none();
    let mut capacity_percent = OptionalF64::none();
    let mut voltage_v = OptionalF64::none();
    let mut temperature_c = OptionalF64::none();
    let mut warning_level = BatteryWarningLevel::Unknown;
    let mut coarse_level = BatteryCoarseLevel::Unknown;
    let mut update_time = OptionalU64::none();

    if let Some(builder) = display_proxy
        && let Ok(proxy) = builder.build().await
    {
        available = true;
        if let Ok(pct) = proxy.percentage().await {
            percentage = pct.clamp(0.0, 100.0) as u8;
        }
        if let Ok(st) = proxy.state().await {
            state = BatteryChargeState::from_u32(st);
        }
        if let Ok(pres) = proxy.is_present().await {
            is_present = pres;
        }
        time_to_empty_secs = optional_positive_i64(proxy.time_to_empty().await);
        time_to_full_secs = optional_positive_i64(proxy.time_to_full().await);
        energy_wh = optional_positive_f64(proxy.energy().await);
        energy_empty_wh = optional_nonnegative_f64(proxy.energy_empty().await);
        energy_full_wh = optional_positive_f64(proxy.energy_full().await);
        energy_full_design_wh = optional_positive_f64(proxy.energy_full_design().await);
        energy_rate_w = optional_positive_f64(proxy.energy_rate().await);
        capacity_percent = optional_percent(proxy.capacity().await);
        voltage_v = optional_positive_f64(proxy.voltage().await);
        temperature_c = optional_positive_f64(proxy.temperature().await);
        warning_level = BatteryWarningLevel::from_u32(proxy.warning_level().await.unwrap_or(0));
        coarse_level = BatteryCoarseLevel::from_u32(proxy.coarse_level().await.unwrap_or(0));
        update_time = optional_positive_u64(proxy.update_time().await);
    }

    let mut physical_devices = Vec::new();
    if let Ok(builder) = UPowerManagerProxy::builder(connection).path(UPOWER_PATH)
        && let Ok(mgr) = builder.build().await
        && let Ok(devices) = mgr.enumerate_devices().await
    {
        for path in devices {
            if let Some(device) = fetch_physical_device_snapshot(connection, &path).await {
                physical_devices.push(device);
            }
        }
    }

    if capacity_percent
        .get()
        .is_none_or(|capacity| capacity <= 0.0)
    {
        capacity_percent = physical_capacity_percent(&physical_devices);
    }

    let system_present = is_present || !physical_devices.is_empty();

    BatteryInfo {
        available,
        is_present: system_present,
        percentage,
        state,
        time_to_full_secs,
        time_to_empty_secs,
        energy_wh,
        energy_empty_wh,
        energy_full_wh,
        energy_full_design_wh,
        energy_rate_w,
        capacity_percent,
        voltage_v,
        temperature_c,
        warning_level,
        coarse_level,
        update_time,
        devices: physical_devices,
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

        // Emit initial snapshot
        ctx.send_replace(fetch_battery_snapshot(&connection).await);

        let manager_builder = match UPowerManagerProxy::builder(&connection).path(UPOWER_PATH) {
            Ok(builder) => builder,
            Err(error) => {
                tracing::debug!(%error, "UPower manager proxy setup failed; retrying");
                ctx.send_replace(BatteryInfo::default());
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            }
        };
        let manager = match manager_builder.build().await {
            Ok(manager) => manager,
            Err(error) => {
                tracing::debug!(%error, "UPower manager proxy failed; retrying");
                ctx.send_replace(BatteryInfo::default());
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            }
        };
        let mut device_added = manager.receive_device_added().await.ok();
        let mut device_removed = manager.receive_device_removed().await.ok();
        let (physical_changed_tx, mut physical_changed_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut physical_watchers = spawn_physical_property_watchers(
            &connection,
            physical_device_paths(&manager).await,
            physical_changed_tx.clone(),
        );

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
                            ctx.send_replace(fetch_battery_snapshot(&connection).await);
                        }
                    }
                    changed = physical_changed_rx.recv() => {
                        if changed.is_none() { break; }
                        ctx.send_replace(fetch_battery_snapshot(&connection).await);
                    }
                    added = async {
                        match device_added.as_mut() {
                            Some(stream) => stream.next().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        if added.is_none() { break; }
                        for watcher in physical_watchers.drain(..) { watcher.abort(); }
                        physical_watchers = spawn_physical_property_watchers(
                            &connection,
                            physical_device_paths(&manager).await,
                            physical_changed_tx.clone(),
                        );
                        ctx.send_replace(fetch_battery_snapshot(&connection).await);
                    }
                    removed = async {
                        match device_removed.as_mut() {
                            Some(stream) => stream.next().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        if removed.is_none() { break; }
                        for watcher in physical_watchers.drain(..) { watcher.abort(); }
                        physical_watchers = spawn_physical_property_watchers(
                            &connection,
                            physical_device_paths(&manager).await,
                            physical_changed_tx.clone(),
                        );
                        ctx.send_replace(fetch_battery_snapshot(&connection).await);
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
                    ctx.send_replace(fetch_battery_snapshot(&connection).await);
                }
            }
        }

        for watcher in physical_watchers {
            watcher.abort();
        }

        // Connection lost or stream terminated
        ctx.send_replace(BatteryInfo::default());
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

/// UPower battery service for tracking battery status and physical devices event-driven.
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
        assert!(!BatteryInfo::default().is_charging());
        assert!(!BatteryInfo::default().is_present);
        assert!(!BatteryInfo::default().available);
    }

    #[test]
    fn test_battery_low_power_threshold_policy() {
        let mut info = BatteryInfo {
            available: true,
            is_present: true,
            percentage: 10,
            state: BatteryChargeState::Discharging,
            ..Default::default()
        };
        assert!(info.is_low_battery());
        assert!(info.low_power_mode());

        info.state = BatteryChargeState::Charging;
        assert!(!info.is_low_battery());
        assert!(!info.low_power_mode());
    }

    #[test]
    fn test_battery_device_payload_sentinels() {
        let dev = BatteryDevicePayload {
            id: "/org/freedesktop/UPower/devices/battery_BAT0".to_string(),
            vendor: "  Dell Inc. ".trim().to_string(),
            model: "Standard".to_string(),
            serial: "".to_string(), // empty string sentinel
            native_path: "/sys/class/power_supply/BAT0".to_string(),
            technology: BatteryTechnology::LithiumIon,
            charge_state: BatteryChargeState::Discharging,
            energy_wh: OptionalF64::some(50.0),
            cycle_count: OptionalU64::none(),
            update_time: OptionalU64::none(),
            ..Default::default()
        };

        assert_eq!(dev.vendor, "Dell Inc.");
        assert!(dev.serial.is_empty());
        assert_eq!(dev.cycle_count.get(), None);
        assert_eq!(dev.update_time.get(), None);
        assert_eq!(dev.energy_wh.get(), Some(50.0));
    }

    #[test]
    fn physical_device_filter_excludes_peripherals_and_absent_batteries() {
        assert!(is_system_battery(2, true, true));
        assert!(!is_system_battery(2, false, true));
        assert!(!is_system_battery(2, true, false));
        assert!(!is_system_battery(5, true, true));
    }

    #[test]
    fn optional_metric_conversion_rejects_upower_sentinels() {
        assert_eq!(optional_positive_i64(Ok(0)).get(), None);
        assert_eq!(optional_positive_i64(Ok(-1)).get(), None);
        assert_eq!(optional_positive_i64(Ok(90)).get(), Some(90));
        assert_eq!(optional_percent(Ok(101.0)).get(), None);
        assert_eq!(optional_percent(Ok(0.0)).get(), Some(0.0));
        assert_eq!(optional_threshold(Ok(u32::MAX)).get(), None);
    }

    #[test]
    fn physical_capacity_prefers_energy_weighted_health() {
        let devices = vec![
            BatteryDevicePayload {
                energy_full_wh: OptionalF64::some(30.0),
                energy_full_design_wh: OptionalF64::some(40.0),
                capacity_percent: OptionalF64::some(1.0),
                ..Default::default()
            },
            BatteryDevicePayload {
                energy_full_wh: OptionalF64::some(20.0),
                energy_full_design_wh: OptionalF64::some(40.0),
                capacity_percent: OptionalF64::some(99.0),
                ..Default::default()
            },
        ];

        assert_eq!(physical_capacity_percent(&devices).get(), Some(62.5));
    }

    #[test]
    fn physical_capacity_falls_back_to_reported_capacity_average() {
        let devices = vec![
            BatteryDevicePayload {
                capacity_percent: OptionalF64::some(60.0),
                ..Default::default()
            },
            BatteryDevicePayload {
                capacity_percent: OptionalF64::some(80.0),
                ..Default::default()
            },
        ];

        assert_eq!(physical_capacity_percent(&devices).get(), Some(70.0));
    }
}
