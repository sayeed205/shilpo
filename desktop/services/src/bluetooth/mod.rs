use anyhow::Result;
use std::collections::HashMap;
use std::process::Command;
use tokio::sync::{mpsc, watch};
use zbus::Connection;

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
    ConnectDevice(String),
    DisconnectDevice(String),
    PairDevice(String),
    RemoveDevice(String),
}

/// Service managing Bluetooth state via DBus (`org.bluez`) with fallback support.
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

                loop {
                    tokio::select! {
                        cmd = command_rx.recv() => {
                            match cmd {
                                Some(cmd) => {
                                    handle_bluetooth_command(&connection_opt, cmd).await;
                                    if let Some(ref conn) = connection_opt
                                        && let Ok(info) = query_bluez_dbus(conn).await
                                    {
                                        let _ = tx_clone.send_replace(info);
                                    }
                                }
                                None => break,
                            }
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {
                            if let Some(ref conn) = connection_opt {
                                if let Ok(info) = query_bluez_dbus(conn).await {
                                    let _ = tx_clone.send_replace(info);
                                }
                            } else {
                                let (available, powered) = query_system_fallback();
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
                            }
                        }
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

    pub fn set_powered(&self, powered: bool) -> bool {
        let mut current = self.info();
        if !current.available && self.command_tx.is_none() {
            return false;
        }

        if let Some(ref tx) = self.command_tx {
            let _ = tx.try_send(BluetoothCommand::SetPowered(powered));
            current.powered = powered;
            self.tx.send_replace(current);
            true
        } else {
            false
        }
    }

    pub fn toggle(&self) -> bool {
        let current = self.info().powered;
        self.set_powered(!current)
    }

    pub fn start_discovery(&self) -> Result<()> {
        if let Some(ref tx) = self.command_tx {
            tx.try_send(BluetoothCommand::StartDiscovery)
                .map_err(|e| anyhow::anyhow!("Failed to send start_discovery command: {e}"))?;
        }
        Ok(())
    }

    pub fn stop_discovery(&self) -> Result<()> {
        if let Some(ref tx) = self.command_tx {
            tx.try_send(BluetoothCommand::StopDiscovery)
                .map_err(|e| anyhow::anyhow!("Failed to send stop_discovery command: {e}"))?;
        }
        Ok(())
    }

    pub fn connect_device(&self, address: &str) -> Result<()> {
        if let Some(ref tx) = self.command_tx {
            tx.try_send(BluetoothCommand::ConnectDevice(address.to_string()))
                .map_err(|e| anyhow::anyhow!("Failed to send connect_device command: {e}"))?;
        }
        Ok(())
    }

    pub fn disconnect_device(&self, address: &str) -> Result<()> {
        if let Some(ref tx) = self.command_tx {
            tx.try_send(BluetoothCommand::DisconnectDevice(address.to_string()))
                .map_err(|e| anyhow::anyhow!("Failed to send disconnect_device command: {e}"))?;
        }
        Ok(())
    }

    pub fn pair_device(&self, address: &str) -> Result<()> {
        if let Some(ref tx) = self.command_tx {
            tx.try_send(BluetoothCommand::PairDevice(address.to_string()))
                .map_err(|e| anyhow::anyhow!("Failed to send pair_device command: {e}"))?;
        }
        Ok(())
    }

    pub fn remove_device(&self, address: &str) -> Result<()> {
        if let Some(ref tx) = self.command_tx {
            tx.try_send(BluetoothCommand::RemoveDevice(address.to_string()))
                .map_err(|e| anyhow::anyhow!("Failed to send remove_device command: {e}"))?;
        }
        Ok(())
    }
}

async fn query_bluez_dbus(conn: &Connection) -> Result<BluetoothInfo> {
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
    let mut devices = Vec::new();

    for (_path, interfaces) in objects {
        if let Some(adapter) = interfaces.get("org.bluez.Adapter1") {
            available = true;
            if let Some(val) = adapter.get("Powered")
                && let Ok(p) = val.downcast_ref::<bool>()
            {
                powered = p;
            }
            if let Some(val) = adapter.get("Discovering")
                && let Ok(d) = val.downcast_ref::<bool>()
            {
                discovering = d;
            }
        }

        if let Some(dev) = interfaces.get("org.bluez.Device1") {
            let address = dev
                .get("Address")
                .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
                .map(|s| s.as_str().to_string())
                .unwrap_or_default();

            let name = dev
                .get("Alias")
                .or_else(|| dev.get("Name"))
                .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
                .map(|s| s.as_str().to_string())
                .unwrap_or_else(|| address.clone());

            let icon = dev
                .get("Icon")
                .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
                .map(|s| s.as_str().to_string());

            let paired = dev
                .get("Paired")
                .and_then(|v| v.downcast_ref::<bool>().ok())
                .unwrap_or(false);

            let connected = dev
                .get("Connected")
                .and_then(|v| v.downcast_ref::<bool>().ok())
                .unwrap_or(false);

            let trusted = dev
                .get("Trusted")
                .and_then(|v| v.downcast_ref::<bool>().ok())
                .unwrap_or(false);

            let blocked = dev
                .get("Blocked")
                .and_then(|v| v.downcast_ref::<bool>().ok())
                .unwrap_or(false);

            let rssi = dev
                .get("RSSI")
                .and_then(|v| v.downcast_ref::<i16>().ok());

            let battery_percentage = interfaces
                .get("org.bluez.Battery1")
                .and_then(|bat| bat.get("Percentage"))
                .and_then(|v| v.downcast_ref::<u8>().ok());

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
    Ok(info)
}

async fn handle_bluetooth_command(
    conn_opt: &Option<Connection>,
    cmd: BluetoothCommand,
) {
    let Some(conn) = conn_opt else {
        if let BluetoothCommand::SetPowered(powered) = cmd {
            let arg = if powered { "on" } else { "off" };
            let _ = Command::new("bluetoothctl").args(["power", arg]).status();
        }
        return;
    };

    match cmd {
        BluetoothCommand::SetPowered(powered) => {
            let _ = conn
                .call_method(
                    Some("org.bluez"),
                    "/org/bluez/hci0",
                    Some("org.freedesktop.DBus.Properties"),
                    "Set",
                    &(
                        "org.bluez.Adapter1",
                        "Powered",
                        zbus::zvariant::Value::Bool(powered),
                    ),
                )
                .await;
        }
        BluetoothCommand::StartDiscovery => {
            let _ = conn
                .call_method(
                    Some("org.bluez"),
                    "/org/bluez/hci0",
                    Some("org.bluez.Adapter1"),
                    "StartDiscovery",
                    &(),
                )
                .await;
        }
        BluetoothCommand::StopDiscovery => {
            let _ = conn
                .call_method(
                    Some("org.bluez"),
                    "/org/bluez/hci0",
                    Some("org.bluez.Adapter1"),
                    "StopDiscovery",
                    &(),
                )
                .await;
        }
        BluetoothCommand::ConnectDevice(address) => {
            let path = device_address_to_path(&address);
            let _ = conn
                .call_method(
                    Some("org.bluez"),
                    path,
                    Some("org.bluez.Device1"),
                    "Connect",
                    &(),
                )
                .await;
        }
        BluetoothCommand::DisconnectDevice(address) => {
            let path = device_address_to_path(&address);
            let _ = conn
                .call_method(
                    Some("org.bluez"),
                    path,
                    Some("org.bluez.Device1"),
                    "Disconnect",
                    &(),
                )
                .await;
        }
        BluetoothCommand::PairDevice(address) => {
            let path = device_address_to_path(&address);
            let _ = conn
                .call_method(
                    Some("org.bluez"),
                    path,
                    Some("org.bluez.Device1"),
                    "Pair",
                    &(),
                )
                .await;
        }
        BluetoothCommand::RemoveDevice(address) => {
            if let Ok(obj_path) =
                zbus::zvariant::ObjectPath::try_from(device_address_to_path(&address))
            {
                let _ = conn
                    .call_method(
                        Some("org.bluez"),
                        "/org/bluez/hci0",
                        Some("org.bluez.Adapter1"),
                        "RemoveDevice",
                        &(obj_path,),
                    )
                    .await;
            }
        }
    }
}

fn device_address_to_path(address: &str) -> String {
    if address.starts_with('/') {
        address.to_string()
    } else {
        format!("/org/bluez/hci0/dev_{}", address.replace(':', "_"))
    }
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
        assert!(!service.toggle());
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
}
