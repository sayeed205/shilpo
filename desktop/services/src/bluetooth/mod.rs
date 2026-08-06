use anyhow::Result;
use std::process::Command;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BluetoothInfo {
    pub powered: bool,
    pub available: bool,
}

use tokio::sync::watch;

pub struct BluetoothService {
    tx: watch::Sender<BluetoothInfo>,
}

impl BluetoothService {
    pub fn new() -> Result<Self> {
        let (available, powered) = Self::query_system();
        let (tx, _) = watch::channel(BluetoothInfo { powered, available });
        Ok(Self { tx })
    }

    pub fn new_offline() -> Self {
        let (tx, _) = watch::channel(BluetoothInfo {
            powered: false,
            available: false,
        });
        Self { tx }
    }

    fn query_system() -> (bool, bool) {
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

    pub fn subscribe(&self) -> watch::Receiver<BluetoothInfo> {
        self.tx.subscribe()
    }

    pub fn info(&self) -> BluetoothInfo {
        self.tx.borrow().clone()
    }

    pub fn set_powered(&self, powered: bool) -> bool {
        let mut current = self.tx.borrow().clone();
        if !current.available {
            return false;
        }

        let arg = if powered { "on" } else { "off" };
        if Command::new("bluetoothctl")
            .args(["power", arg])
            .status()
            .is_ok_and(|s| s.success())
        {
            current.powered = powered;
            let _ = self.tx.send_replace(current);
            true
        } else {
            let rf_arg = if powered { "unblock" } else { "block" };
            if Command::new("rfkill")
                .args([rf_arg, "bluetooth"])
                .status()
                .is_ok_and(|s| s.success())
            {
                current.powered = powered;
                let _ = self.tx.send_replace(current);
                true
            } else {
                false
            }
        }
    }

    pub fn toggle(&self) -> bool {
        let current = self.tx.borrow().powered;
        self.set_powered(!current)
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
        assert!(!service.toggle());
    }
}
