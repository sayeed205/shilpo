use anyhow::Result;
use futures_lite::StreamExt;
use std::collections::HashMap;
use std::process::Command;
use tokio::sync::{mpsc, watch};
use zbus::Connection;

/// Strongly typed MAC address wrapper for Bluetooth devices.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash, serde::Serialize, serde::Deserialize)]
pub struct BluetoothAddress(pub String);

impl BluetoothAddress {
    pub fn new(address: impl Into<String>) -> Self {
        Self(address.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_dbus_path(&self, adapter_path: &str) -> String {
        if self.0.starts_with('/') {
            self.0.clone()
        } else {
            format!("{adapter_path}/dev_{}", self.0.replace(':', "_"))
        }
    }
}

impl From<&str> for BluetoothAddress {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for BluetoothAddress {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Represents a remote Bluetooth device managed by BlueZ (`org.bluez.Device1`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct BluetoothDevice {
    pub address: String,
    pub name: String,
    pub icon: Option<String>,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    pub blocked: bool,
    pub rssi: Option<i16>,
    pub battery_percentage: Option<u8>,
}

/// Consolidated snapshot of the Bluetooth subsystem state.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BluetoothInfo {
    pub available: bool,
    pub powered: bool,
    pub discovering: bool,
    pub connected_devices_count: usize,
    pub connected: bool,
    pub devices: Vec<BluetoothDevice>,
}

impl BluetoothInfo {
    pub fn update_derived_fields(&mut self) {
        self.connected_devices_count = self.devices.iter().filter(|d| d.connected).count();
        self.connected = self.connected_devices_count > 0;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BluetoothCommand {
    SetPowered(bool),
    StartDiscovery,
    StopDiscovery,
    ConnectDevice(BluetoothAddress),
    DisconnectDevice(BluetoothAddress),
    PairDevice(BluetoothAddress),
    RemoveDevice(BluetoothAddress),
}

/// Service managing Bluetooth state via DBus (`org.bluez`) with event-driven signal reception.
pub struct BluetoothService {
    tx: watch::Sender<BluetoothInfo>,
    _task: Option<tokio::task::JoinHandle<()>>,
    command_tx: Option<mpsc::Sender<BluetoothCommand>>,
}

impl Drop for BluetoothService {
    fn drop(&mut self) {
        if let Some(task) = self._task.take() {
            task.abort();
        }
    }
}

impl BluetoothService {
    pub fn new_offline() -> Self {
        let (tx, _) = watch::channel(BluetoothInfo::default());
        Self {
            tx,
            _task: None,
            command_tx: None,
        }
    }

    pub fn new() -> Result<Self> {
        let (tx, _) = watch::channel(BluetoothInfo::default());
        let (command_tx, mut command_rx) = mpsc::channel::<BluetoothCommand>(32);

        let tx_clone = tx.clone();
        let task = if tokio::runtime::Handle::try_current().is_ok() {
            Some(tokio::spawn(async move {
                let connection_opt = Connection::system().await.ok();
                let mut current_adapter_path: Option<String> = None;

                if let Some(ref conn) = connection_opt {
                    let _ = conn
                        .call_method(
                            Some("org.freedesktop.DBus"),
                            "/org/freedesktop/DBus",
                            Some("org.freedesktop.DBus"),
                            "AddMatch",
                            &("type='signal',sender='org.bluez'",),
                        )
                        .await;

                    update_dbus_state(conn, &tx_clone, &mut current_adapter_path).await;

                    let mut stream = zbus::MessageStream::from(conn);

                    loop {
                        tokio::select! {
                            cmd = command_rx.recv() => {
                                match cmd {
                                    Some(cmd) => {
                                        handle_bluetooth_command(&connection_opt, current_adapter_path.as_deref(), cmd).await;
                                        update_dbus_state(conn, &tx_clone, &mut current_adapter_path).await;
                                    }
                                    None => break,
                                }
                            }
                            Some(msg_res) = stream.next() => {
                                if let Ok(msg) = msg_res && is_bluez_signal(&msg) {
                                    update_dbus_state(conn, &tx_clone, &mut current_adapter_path).await;
                                }
                            }
                        }
                    }
                } else {
                    let (available, powered) = check_rfkill_bluetooth_status();
                    let mut info = BluetoothInfo {
                        available,
                        powered,
                        discovering: false,
                        connected_devices_count: 0,
                        connected: false,
                        devices: Vec::new(),
                    };
                    info.update_derived_fields();
                    let _ = tx_clone.send_replace(info);

                    while let Some(cmd) = command_rx.recv().await {
                        handle_bluetooth_command(&None, None, cmd).await;
                        let (available, powered) = check_rfkill_bluetooth_status();
                        let mut info = tx_clone.borrow().clone();
                        info.available = available;
                        info.powered = powered;
                        info.update_derived_fields();
                        let _ = tx_clone.send_replace(info);
                    }
                }
            }))
        } else {
            None
        };

        Ok(Self {
            tx,
            _task: task,
            command_tx: Some(command_tx),
        })
    }

    pub fn subscribe(&self) -> watch::Receiver<BluetoothInfo> {
        self.tx.subscribe()
    }

    pub fn info(&self) -> BluetoothInfo {
        self.tx.borrow().clone()
    }

    fn send_command(&self, cmd: BluetoothCommand) -> Result<()> {
        if let Some(ref tx) = self.command_tx {
            tx.try_send(cmd)
                .map_err(|e| anyhow::anyhow!("Failed to send bluetooth command: {e}"))?;
            Ok(())
        } else {
            anyhow::bail!("Bluetooth service command channel is not initialized")
        }
    }

    pub fn set_powered(&self, powered: bool) -> Result<()> {
        if !self.info().available && self.command_tx.is_none() {
            anyhow::bail!("Bluetooth service unavailable or running offline");
        }
        self.send_command(BluetoothCommand::SetPowered(powered))
    }

    pub fn toggle(&self) -> Result<bool> {
        let current = self.info().powered;
        let target = !current;
        self.set_powered(target)?;
        Ok(target)
    }

    pub fn start_discovery(&self) -> Result<()> {
        self.send_command(BluetoothCommand::StartDiscovery)
    }

    pub fn stop_discovery(&self) -> Result<()> {
        self.send_command(BluetoothCommand::StopDiscovery)
    }

    pub fn connect_device(&self, address: impl Into<BluetoothAddress>) -> Result<()> {
        self.send_command(BluetoothCommand::ConnectDevice(address.into()))
    }

    pub fn disconnect_device(&self, address: impl Into<BluetoothAddress>) -> Result<()> {
        self.send_command(BluetoothCommand::DisconnectDevice(address.into()))
    }

    pub fn pair_device(&self, address: impl Into<BluetoothAddress>) -> Result<()> {
        self.send_command(BluetoothCommand::PairDevice(address.into()))
    }

    pub fn remove_device(&self, address: impl Into<BluetoothAddress>) -> Result<()> {
        self.send_command(BluetoothCommand::RemoveDevice(address.into()))
    }
}

async fn update_dbus_state(
    conn: &Connection,
    tx: &watch::Sender<BluetoothInfo>,
    current_adapter_path: &mut Option<String>,
) {
    if let Ok((info, adapter_path)) = query_bluez_dbus(conn).await {
        *current_adapter_path = adapter_path;
        let _ = tx.send_replace(info);
    }
}

pub fn is_bluez_signal(msg: &zbus::Message) -> bool {
    if msg.message_type() != zbus::message::Type::Signal {
        return false;
    }
    if let Some(iface) = msg.header().interface() {
        let name = iface.as_str();
        name == "org.freedesktop.DBus.ObjectManager"
            || name == "org.freedesktop.DBus.Properties"
            || name == "org.bluez.Adapter1"
            || name == "org.bluez.Device1"
    } else {
        false
    }
}

fn get_property<T: zbus::zvariant::DynamicType + TryFrom<zbus::zvariant::OwnedValue>>(
    props: &HashMap<String, zbus::zvariant::OwnedValue>,
    key: &str,
) -> Option<T> {
    props.get(key).and_then(|v| T::try_from(v.clone()).ok())
}

fn get_str(props: &HashMap<String, zbus::zvariant::OwnedValue>, key: &str) -> Option<String> {
    props
        .get(key)
        .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
        .map(|s| s.as_str().to_string())
}

fn get_bool(props: &HashMap<String, zbus::zvariant::OwnedValue>, key: &str) -> bool {
    get_property::<bool>(props, key).unwrap_or(false)
}

async fn query_bluez_dbus(conn: &Connection) -> Result<(BluetoothInfo, Option<String>)> {
    let reply = conn
        .call_method(
            Some("org.bluez"),
            "/",
            Some("org.freedesktop.DBus.ObjectManager"),
            "GetManagedObjects",
            &(),
        )
        .await?;

    type ManagedObjects = HashMap<
        zbus::zvariant::OwnedObjectPath,
        HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>,
    >;

    let objects: ManagedObjects = reply.body().deserialize()?;

    let mut powered = false;
    let mut discovering = false;
    let mut available = false;
    let mut detected_adapter_path: Option<String> = None;
    let mut devices = Vec::new();

    for (path, interfaces) in objects {
        if let Some(adapter) = interfaces.get("org.bluez.Adapter1") {
            available = true;
            if detected_adapter_path.is_none() {
                detected_adapter_path = Some(path.as_str().to_string());
            }
            powered = get_bool(adapter, "Powered");
            discovering = get_bool(adapter, "Discovering");
        }

        if let Some(dev) = interfaces.get("org.bluez.Device1") {
            let address = get_str(dev, "Address").unwrap_or_default();
            let name = get_str(dev, "Alias")
                .or_else(|| get_str(dev, "Name"))
                .unwrap_or_else(|| address.clone());
            let icon = get_str(dev, "Icon");
            let paired = get_bool(dev, "Paired");
            let connected = get_bool(dev, "Connected");
            let trusted = get_bool(dev, "Trusted");
            let blocked = get_bool(dev, "Blocked");
            let rssi = get_property::<i16>(dev, "RSSI");

            let battery_percentage = interfaces
                .get("org.bluez.Battery1")
                .and_then(|bat| get_property::<u8>(bat, "Percentage"));

            devices.push(BluetoothDevice {
                address,
                name,
                icon,
                paired,
                connected,
                trusted,
                blocked,
                rssi,
                battery_percentage,
            });
        }
    }

    let mut info = BluetoothInfo {
        available,
        powered,
        discovering,
        connected_devices_count: 0,
        connected: false,
        devices,
    };
    info.update_derived_fields();
    Ok((info, detected_adapter_path))
}

async fn call_bluez<T: zbus::zvariant::DynamicType + serde::Serialize>(
    conn: &Connection,
    path: &str,
    interface: &str,
    member: &str,
    args: &T,
) -> Result<()> {
    conn.call_method(Some("org.bluez"), path, Some(interface), member, args)
        .await?;
    Ok(())
}

async fn handle_bluetooth_command(
    conn_opt: &Option<Connection>,
    adapter_path_opt: Option<&str>,
    cmd: BluetoothCommand,
) {
    let adapter_path = adapter_path_opt.unwrap_or("/org/bluez/hci0");

    let Some(conn) = conn_opt else {
        if let BluetoothCommand::SetPowered(powered) = cmd {
            let arg = if powered { "on" } else { "off" };
            if let Err(err) = Command::new("bluetoothctl").args(["power", arg]).status() {
                tracing::warn!("Fallback bluetoothctl power failed: {err}");
            }
        } else {
            tracing::warn!(
                "Bluetooth DBus connection unavailable for command: {:?}",
                cmd
            );
        }
        return;
    };

    let res: Result<()> = match cmd {
        BluetoothCommand::SetPowered(powered) => conn
            .call_method(
                Some("org.bluez"),
                adapter_path,
                Some("org.freedesktop.DBus.Properties"),
                "Set",
                &(
                    "org.bluez.Adapter1",
                    "Powered",
                    zbus::zvariant::Value::Bool(powered),
                ),
            )
            .await
            .map(|_| ())
            .map_err(Into::into),
        BluetoothCommand::StartDiscovery => {
            call_bluez(
                conn,
                adapter_path,
                "org.bluez.Adapter1",
                "StartDiscovery",
                &(),
            )
            .await
        }
        BluetoothCommand::StopDiscovery => {
            call_bluez(
                conn,
                adapter_path,
                "org.bluez.Adapter1",
                "StopDiscovery",
                &(),
            )
            .await
        }
        BluetoothCommand::ConnectDevice(address) => {
            let path = address.to_dbus_path(adapter_path);
            call_bluez(conn, &path, "org.bluez.Device1", "Connect", &()).await
        }
        BluetoothCommand::DisconnectDevice(address) => {
            let path = address.to_dbus_path(adapter_path);
            call_bluez(conn, &path, "org.bluez.Device1", "Disconnect", &()).await
        }
        BluetoothCommand::PairDevice(address) => {
            let path = address.to_dbus_path(adapter_path);
            call_bluez(conn, &path, "org.bluez.Device1", "Pair", &()).await
        }
        BluetoothCommand::RemoveDevice(address) => {
            let path_str = address.to_dbus_path(adapter_path);
            if let Ok(obj_path) = zbus::zvariant::ObjectPath::try_from(path_str) {
                call_bluez(
                    conn,
                    adapter_path,
                    "org.bluez.Adapter1",
                    "RemoveDevice",
                    &(obj_path,),
                )
                .await
            } else {
                Err(anyhow::anyhow!(
                    "Invalid DBus object path for address: {}",
                    address.as_str()
                ))
            }
        }
    };

    if let Err(err) = res {
        tracing::warn!("Failed DBus command execution: {err}");
    }
}

pub fn check_rfkill_bluetooth_status() -> (bool, bool) {
    if let Ok(entries) = std::fs::read_dir("/sys/class/rfkill") {
        let mut found = false;
        let mut blocked = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(rf_type) = std::fs::read_to_string(path.join("type"))
                && rf_type.trim() == "bluetooth"
            {
                found = true;
                let soft = std::fs::read_to_string(path.join("soft")).unwrap_or_default();
                let hard = std::fs::read_to_string(path.join("hard")).unwrap_or_default();
                if soft.trim() == "1" || hard.trim() == "1" {
                    blocked = true;
                }
            }
        }
        if found {
            return (true, !blocked);
        }
    }
    query_system_fallback()
}

fn query_system_fallback() -> (bool, bool) {
    if let Ok(output) = Command::new("bluetoothctl").arg("show").output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let powered = text.lines().any(|l| l.trim().starts_with("Powered: yes"));
        (true, powered)
    } else if let Ok(output) = Command::new("rfkill").args(["list", "bluetooth"]).output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let blocked = text.lines().any(|l| l.contains("Soft blocked: yes"));
        (true, !blocked)
    } else {
        (false, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluetooth_offline() {
        let service = BluetoothService::new_offline();
        let info = service.info();
        assert!(!info.available);
        assert!(!info.powered);
        assert!(!info.connected);
        assert_eq!(info.connected_devices_count, 0);
        assert!(info.devices.is_empty());
        assert!(service.toggle().is_err());
    }

    #[test]
    fn test_bluetooth_derived_fields() {
        let mut info = BluetoothInfo {
            available: true,
            powered: true,
            discovering: false,
            connected_devices_count: 0,
            connected: false,
            devices: vec![
                BluetoothDevice {
                    address: "00:11:22:33:44:55".into(),
                    name: "Headphones".into(),
                    connected: true,
                    paired: true,
                    ..Default::default()
                },
                BluetoothDevice {
                    address: "AA:BB:CC:DD:EE:FF".into(),
                    name: "Mouse".into(),
                    connected: false,
                    paired: true,
                    ..Default::default()
                },
            ],
        };

        info.update_derived_fields();
        assert!(info.connected);
        assert_eq!(info.connected_devices_count, 1);
    }

    #[test]
    fn test_bluetooth_address_path() {
        let addr = BluetoothAddress::new("00:11:22:33:44:55");
        assert_eq!(
            addr.to_dbus_path("/org/bluez/hci1"),
            "/org/bluez/hci1/dev_00_11_22_33_44_55"
        );

        let path_addr = BluetoothAddress::new("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF");
        assert_eq!(
            path_addr.to_dbus_path("/org/bluez/hci0"),
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF"
        );
    }

    #[test]
    fn test_rfkill_status_check() {
        let (avail, _powered) = check_rfkill_bluetooth_status();
        let _ = avail;
    }
}
