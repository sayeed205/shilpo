use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, watch};
use tracing::{debug, warn};

/// Screen brightness status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrightnessInfo {
    pub percentage: u8,
    pub available: bool,
    pub device_name: Option<String>,
}

impl BrightnessInfo {
    /// Converts a perceptual linear slider percentage (0..=100) to logarithmic raw percentage (0..=100) via quadratic curve scaling.
    pub fn perceptual_percent_to_raw(percent: u8) -> u8 {
        let p = (percent.min(100) as f64) / 100.0;
        let log_scaled = p * p;
        (log_scaled * 100.0).round() as u8
    }

    /// Converts a raw percentage (0..=100) back to perceptual linear slider percentage (0..=100).
    pub fn raw_to_perceptual_percent(raw: u8) -> u8 {
        let r = (raw.min(100) as f64) / 100.0;
        let linear = r.sqrt();
        (linear * 100.0).round() as u8
    }
}

/// Backlight device hardware type priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BacklightType {
    Firmware = 1, // ACPI fallback (e.g. acpi_video0)
    Platform = 2, // Platform driver (e.g. thinkpad_screen, apple_backlight)
    Raw = 3,      // GPU-driven raw control (e.g. intel_backlight, amdgpu_bl0, nvidia_0)
    Unknown = 0,
}

impl BacklightType {
    pub fn from_type_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "raw" => Self::Raw,
            "platform" => Self::Platform,
            "firmware" => Self::Firmware,
            _ => Self::Unknown,
        }
    }
}

/// Discovered backlight device representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklightDevice {
    pub name: String,
    pub sysfs_path: PathBuf,
    pub device_type: BacklightType,
}

impl BacklightDevice {
    /// Computes a priority score for backlight device selection.
    pub fn priority_score(&self) -> i32 {
        let mut score = match self.device_type {
            BacklightType::Raw => 30,
            BacklightType::Platform => 20,
            BacklightType::Firmware => 10,
            BacklightType::Unknown => 15,
        };

        let lower = self.name.to_lowercase();
        if lower.contains("intel")
            || lower.contains("amd")
            || lower.contains("nvidia")
            || lower.contains("radeon")
            || lower.contains("nouveau")
        {
            score += 5;
        }
        if lower.starts_with("acpi_video") {
            score -= 5;
        }
        score
    }
}

/// Discovers primary backlight device from sysfs directory.
pub fn discover_primary_backlight(sysfs_base: &Path) -> Option<BacklightDevice> {
    let entries = std::fs::read_dir(sysfs_base).ok()?;
    let mut devices = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !path.join("brightness").exists() || !path.join("max_brightness").exists() {
            continue;
        }

        let device_type = if let Ok(type_str) = std::fs::read_to_string(path.join("type")) {
            BacklightType::from_type_str(&type_str)
        } else {
            BacklightType::Unknown
        };

        devices.push(BacklightDevice {
            name,
            sysfs_path: path,
            device_type,
        });
    }

    devices.sort_by(|a, b| {
        b.priority_score()
            .cmp(&a.priority_score())
            .then_with(|| a.name.cmp(&b.name))
    });

    devices.into_iter().next()
}

/// Reads current raw brightness, max brightness, and calculates percentage.
pub fn read_sysfs_brightness(sysfs_path: &Path) -> Option<(u8, u32, u32)> {
    let curr_str = std::fs::read_to_string(sysfs_path.join("brightness")).ok()?;
    let max_str = std::fs::read_to_string(sysfs_path.join("max_brightness")).ok()?;

    let curr: f64 = curr_str.trim().parse().ok()?;
    let max: f64 = max_str.trim().parse().ok()?;

    if max <= 0.0 {
        return Some((0, 0, 0));
    }

    let percentage = ((curr / max) * 100.0).round() as u8;
    Some((percentage.min(100), curr as u32, max as u32))
}

/// System screen brightness service.
pub struct BrightnessService {
    tx: watch::Sender<BrightnessInfo>,
    rx: watch::Receiver<BrightnessInfo>,
    cmd_tx: Option<mpsc::UnboundedSender<u8>>,
    #[allow(dead_code)]
    max_brightness: u32,
}

impl BrightnessService {
    pub fn new() -> Result<Self> {
        Self::new_with_sysfs_path(Path::new("/sys/class/backlight"))
    }

    pub fn new_offline() -> Self {
        let (tx, rx) = watch::channel(BrightnessInfo::default());
        Self {
            tx,
            rx,
            cmd_tx: None,
            max_brightness: 100,
        }
    }

    pub fn new_with_sysfs_path(sysfs_base: &Path) -> Result<Self> {
        let device = discover_primary_backlight(sysfs_base)
            .context("no compatible backlight device found in sysfs")?;

        let (initial_pct, _curr, max) = read_sysfs_brightness(&device.sysfs_path)
            .context("failed to read backlight sysfs attributes")?;

        let initial_info = BrightnessInfo {
            percentage: initial_pct,
            available: true,
            device_name: Some(device.name.clone()),
        };

        let (tx, rx) = watch::channel(initial_info);
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<u8>();

        let device_name = device.name.clone();
        let sysfs_path = device.sysfs_path.clone();

        // Spawn DBus command handler task
        tokio::spawn(async move {
            let dbus_conn = match zbus::Connection::system().await {
                Ok(conn) => Some(conn),
                Err(err) => {
                    warn!(error = %err, "failed to connect to system DBus bus for logind brightness control");
                    None
                }
            };

            while let Some(target_pct) = cmd_rx.recv().await {
                let target_pct = target_pct.min(100);
                let target_raw = ((target_pct as u64 * max as u64) as f64 / 100.0).round() as u32;

                if let Some(ref conn) = dbus_conn {
                    let res = conn
                        .call_method(
                            Some("org.freedesktop.login1"),
                            "/org/freedesktop/login1/session/auto",
                            Some("org.freedesktop.login1.Session"),
                            "SetBrightness",
                            &("backlight", device_name.as_str(), target_raw),
                        )
                        .await;
                    if let Err(err) = res {
                        warn!(error = %err, device = %device_name, target_raw, "failed to set brightness via systemd-logind DBus");
                    }
                }
            }
        });

        // Spawn non-Send udev event monitor in dedicated thread
        let tx_clone = tx.clone();
        let sysfs_path_clone = sysfs_path.clone();
        let device_name_clone = device.name.clone();

        let _ = std::thread::Builder::new()
            .name("shilpo-udev-backlight".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(err) => {
                        warn!(error = %err, "failed to build tokio runtime for udev thread");
                        return;
                    }
                };

                rt.block_on(async move {
                    let local_set = tokio::task::LocalSet::new();
                    local_set
                        .run_until(async move {
                            let mut udev_stream = match tokio_udev::MonitorBuilder::new()
                                .and_then(|builder| builder.match_subsystem("backlight"))
                                .and_then(|builder| builder.listen())
                                .and_then(tokio_udev::AsyncMonitorSocket::new)
                            {
                                Ok(stream) => stream,
                                Err(err) => {
                                    warn!(error = %err, "failed to initialize tokio-udev monitor socket");
                                    return;
                                }
                            };

                            use futures_lite::StreamExt;
                            while let Some(res) = udev_stream.next().await {
                                if res.is_ok()
                                    && let Some((pct, _curr, _max)) = read_sysfs_brightness(&sysfs_path_clone)
                                {
                                    debug!(device = %device_name_clone, percentage = pct, "backlight uevent received; updated brightness");
                                    tx_clone.send_if_modified(|curr| {
                                        if curr.percentage != pct || !curr.available {
                                            curr.percentage = pct;
                                            curr.available = true;
                                            true
                                        } else {
                                            false
                                        }
                                    });
                                }
                            }
                        })
                        .await;
                });
            });

        Ok(Self {
            tx,
            rx,
            cmd_tx: Some(cmd_tx),
            max_brightness: max,
        })
    }

    pub fn subscribe(&self) -> watch::Receiver<BrightnessInfo> {
        self.rx.clone()
    }

    pub fn brightness_info(&self) -> BrightnessInfo {
        self.rx.borrow().clone()
    }

    pub fn set_brightness(&self, percentage: u8) {
        let percentage = percentage.min(100);
        let mut current = self.brightness_info();
        if !current.available {
            warn!("brightness backend unavailable; ignoring brightness change");
            return;
        }

        current.percentage = percentage;
        let _ = self.tx.send(current);

        if let Some(ref cmd_tx) = self.cmd_tx {
            let _ = cmd_tx.send(percentage);
        }
    }

    pub fn set_brightness_smooth(&self, target_percentage: u8) {
        let target = target_percentage.min(100);
        let log_target = BrightnessInfo::perceptual_percent_to_raw(target);
        self.set_brightness(log_target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_is_unavailable() {
        assert_eq!(BrightnessInfo::default().percentage, 0);
        assert!(!BrightnessInfo::default().available);
        assert_eq!(BrightnessInfo::default().device_name, None);
    }

    #[test]
    fn test_logarithmic_brightness_curves_and_stepping() {
        assert_eq!(BrightnessInfo::perceptual_percent_to_raw(0), 0);
        assert_eq!(BrightnessInfo::perceptual_percent_to_raw(100), 100);
        assert_eq!(BrightnessInfo::perceptual_percent_to_raw(50), 25);
        assert_eq!(BrightnessInfo::raw_to_perceptual_percent(25), 50);
    }

    #[test]
    fn test_backlight_prioritization() {
        let temp_dir = TempDir::new().unwrap();
        let sysfs_base = temp_dir.path();

        // Create firmware backlight: acpi_video0
        let acpi_dir = sysfs_base.join("acpi_video0");
        std::fs::create_dir(&acpi_dir).unwrap();
        std::fs::write(acpi_dir.join("brightness"), "10\n").unwrap();
        std::fs::write(acpi_dir.join("max_brightness"), "100\n").unwrap();
        std::fs::write(acpi_dir.join("type"), "firmware\n").unwrap();

        // Create raw backlight: intel_backlight
        let intel_dir = sysfs_base.join("intel_backlight");
        std::fs::create_dir(&intel_dir).unwrap();
        std::fs::write(intel_dir.join("brightness"), "150\n").unwrap();
        std::fs::write(intel_dir.join("max_brightness"), "255\n").unwrap();
        std::fs::write(intel_dir.join("type"), "raw\n").unwrap();

        let device = discover_primary_backlight(sysfs_base).expect("should find device");
        assert_eq!(device.name, "intel_backlight");
        assert_eq!(device.device_type, BacklightType::Raw);

        let (pct, curr, max) = read_sysfs_brightness(&intel_dir).unwrap();
        assert_eq!(curr, 150);
        assert_eq!(max, 255);
        assert_eq!(pct, 59);
    }

    #[tokio::test]
    async fn test_brightness_service_with_mock_sysfs() {
        let temp_dir = TempDir::new().unwrap();
        let sysfs_base = temp_dir.path();

        let amd_dir = sysfs_base.join("amdgpu_bl0");
        std::fs::create_dir(&amd_dir).unwrap();
        std::fs::write(amd_dir.join("brightness"), "50\n").unwrap();
        std::fs::write(amd_dir.join("max_brightness"), "100\n").unwrap();
        std::fs::write(amd_dir.join("type"), "raw\n").unwrap();

        let service = BrightnessService::new_with_sysfs_path(sysfs_base).unwrap();
        let info = service.brightness_info();
        assert!(info.available);
        assert_eq!(info.percentage, 50);
        assert_eq!(info.device_name.as_deref(), Some("amdgpu_bl0"));

        service.set_brightness(75);
        assert_eq!(service.brightness_info().percentage, 75);
    }

    #[tokio::test]
    async fn test_offline_brightness_service() {
        let service = BrightnessService::new_offline();
        let info = service.brightness_info();
        assert!(!info.available);
        assert_eq!(info.percentage, 0);
    }
}
