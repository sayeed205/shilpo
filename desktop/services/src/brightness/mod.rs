use anyhow::Result;
use ddc::*;
use ddc_i2c::from_i2c_device;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, watch};
use tracing::{debug, warn};

/// VCP luminance (screen brightness) feature code standard.
pub const VCP_LUMINANCE: u8 = 0x10;

/// Brightness control hardware backend type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum BrightnessBackend {
    SysfsLogind { sysfs_path: PathBuf, device_name: String },
    DdcCiDirect { bus_index: u8 },
}

/// Discovered display brightness device representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayBrightnessInfo {
    pub id: String,                 // e.g. "sysfs:intel_backlight" or "ddc:i2c-3"
    pub name: String,               // Friendly display model / device name
    pub connector: Option<String>,   // DRM connector name matching Wayland/Niri output (e.g. "eDP-1", "DP-1")
    pub percentage: u8,             // Perceptual brightness percentage (0..=100)
    pub is_primary: bool,
    pub backend: BrightnessBackend,
}

/// Screen brightness status (multi-display aware with backward compatibility).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrightnessInfo {
    pub percentage: u8,
    pub available: bool,
    pub device_name: Option<String>,
    pub displays: Vec<DisplayBrightnessInfo>,
    pub primary_display_id: Option<String>,
    pub permissions_ok: bool,
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

/// Sysfs brightness readings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysfsBrightness {
    pub percentage: u8,
    pub current_raw: u32,
    pub max_raw: u32,
}

/// Reads current raw brightness, max brightness, and calculates percentage.
pub fn read_sysfs_brightness(sysfs_path: &Path) -> Option<SysfsBrightness> {
    let curr_str = std::fs::read_to_string(sysfs_path.join("brightness")).ok()?;
    let max_str = std::fs::read_to_string(sysfs_path.join("max_brightness")).ok()?;

    let curr: f64 = curr_str.trim().parse().ok()?;
    let max: f64 = max_str.trim().parse().ok()?;

    if max <= 0.0 {
        return Some(SysfsBrightness {
            percentage: 0,
            current_raw: 0,
            max_raw: 0,
        });
    }

    let percentage = ((curr / max) * 100.0).round() as u8;
    Some(SysfsBrightness {
        percentage: percentage.min(100),
        current_raw: curr as u32,
        max_raw: max as u32,
    })
}

/// Helper to scan DRM connector mapping from `/sys/class/drm/`
pub fn discover_drm_connectors() -> HashMap<u8, String> {
    let mut map = HashMap::new();
    let drm_dir = Path::new("/sys/class/drm");
    if let Ok(entries) = std::fs::read_dir(drm_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if let Some(idx) = name.find('-') {
                let connector = &name[idx + 1..];
                let ddc_path = path.join("ddc");
                if let Ok(target) = std::fs::read_link(&ddc_path)
                    && let Some(target_str) = target.to_str()
                    && let Some(i2c_idx) = target_str.rfind("i2c-")
                    && let Ok(bus) = target_str[i2c_idx + 4..].parse::<u8>()
                {
                    map.insert(bus, connector.to_string());
                }
            }
        }
    }
    map
}

/// Discovers existing `/dev/i2c-*` device node paths dynamically.
pub fn discover_i2c_bus_paths() -> Vec<(u8, PathBuf)> {
    let mut buses = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(num_str) = name_str.strip_prefix("i2c-")
                && let Ok(bus) = num_str.parse::<u8>()
            {
                buses.push((bus, entry.path()));
            }
        }
    }
    buses.sort_by_key(|(bus, _)| *bus);
    buses
}

/// Probes `/dev/i2c-*` devices for DDC/CI compatible external displays.
pub fn discover_ddc_displays() -> (Vec<DisplayBrightnessInfo>, bool) {
    let mut displays = Vec::new();
    let mut permissions_ok = true;
    let connector_map = discover_drm_connectors();
    let buses = discover_i2c_bus_paths();

    for (bus, path) in buses {
        if std::fs::File::options().read(true).write(true).open(&path).is_err() {
            permissions_ok = false;
        }

        if let Ok(mut dev) = from_i2c_device(&path) {
            if let Ok(vcp) = dev.get_vcp_feature(VCP_LUMINANCE) {
                let max = vcp.maximum();
                let curr = vcp.value();
                let percentage = if max > 0 {
                    ((curr as f64 / max as f64) * 100.0).round() as u8
                } else {
                    0
                };

                let connector = connector_map.get(&bus).cloned();
                let name = connector
                    .as_ref()
                    .map(|c| format!("External Display ({})", c))
                    .unwrap_or_else(|| format!("External Display (i2c-{})", bus));

                displays.push(DisplayBrightnessInfo {
                    id: format!("ddc:i2c-{}", bus),
                    name,
                    connector,
                    percentage: percentage.min(100),
                    is_primary: false,
                    backend: BrightnessBackend::DdcCiDirect { bus_index: bus },
                });
            }
        }
    }

    (displays, permissions_ok)
}

/// Abstracted trait for setting brightness on hardware via DBus or IPC.
pub trait BrightnessSetter: Send + Sync + 'static {
    fn set_brightness(
        &self,
        device_name: &str,
        target_raw: u32,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Resilient systemd-logind DBus brightness setter with dynamic reconnection.
pub struct LogindDbusSetter {
    conn: Mutex<Option<zbus::Connection>>,
}

impl LogindDbusSetter {
    pub fn new() -> Self {
        Self {
            conn: Mutex::new(None),
        }
    }

    async fn get_or_connect(&self) -> Option<zbus::Connection> {
        let mut guard = self.conn.lock().await;
        if let Some(ref conn) = *guard
            && !conn.is_closed()
        {
            return Some(conn.clone());
        }
        match zbus::Connection::system().await {
            Ok(conn) => {
                *guard = Some(conn.clone());
                Some(conn)
            }
            Err(err) => {
                warn!(error = %err, "failed to connect to system DBus bus for logind brightness control");
                *guard = None;
                None
            }
        }
    }
}

impl Default for LogindDbusSetter {
    fn default() -> Self {
        Self::new()
    }
}

impl BrightnessSetter for LogindDbusSetter {
    async fn set_brightness(&self, device_name: &str, target_raw: u32) -> Result<()> {
        if let Some(conn) = self.get_or_connect().await {
            let res = conn
                .call_method(
                    Some("org.freedesktop.login1"),
                    "/org/freedesktop/login1/session/auto",
                    Some("org.freedesktop.login1.Session"),
                    "SetBrightness",
                    &("backlight", device_name, target_raw),
                )
                .await;
            if let Err(err) = res {
                warn!(error = %err, device = %device_name, target_raw, "failed to set brightness via systemd-logind DBus");
                let mut guard = self.conn.lock().await;
                *guard = None;
                anyhow::bail!(err);
            }
            Ok(())
        } else {
            anyhow::bail!("system DBus connection unavailable")
        }
    }
}

/// Abstracted trait for monitoring udev backlight kernel events.
pub trait UdevMonitor: Send + Sync + 'static {
    fn listen(
        &self,
        tx: watch::Sender<BrightnessInfo>,
        sysfs_path: PathBuf,
        device_name: String,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Option<std::thread::JoinHandle<()>>;
}

/// Linux netlink udev backlight event monitor thread.
pub struct SystemUdevMonitor;

impl UdevMonitor for SystemUdevMonitor {
    fn listen(
        &self,
        tx: watch::Sender<BrightnessInfo>,
        sysfs_path: PathBuf,
        device_name: String,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> Option<std::thread::JoinHandle<()>> {
        std::thread::Builder::new()
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
                            loop {
                                tokio::select! {
                                    _ = shutdown_rx.changed() => {
                                        if *shutdown_rx.borrow() {
                                            break;
                                        }
                                    }
                                    res = udev_stream.next() => {
                                        match res {
                                            Some(Ok(_)) => {
                                                if let Some(sysfs) = read_sysfs_brightness(&sysfs_path) {
                                                    debug!(device = %device_name, percentage = sysfs.percentage, "backlight uevent received; updated brightness");
                                                    tx.send_if_modified(|curr| {
                                                        if curr.percentage != sysfs.percentage || !curr.available {
                                                            curr.percentage = sysfs.percentage;
                                                            curr.available = true;
                                                            if let Some(ref pid) = curr.primary_display_id {
                                                                if let Some(disp) = curr.displays.iter_mut().find(|d| d.id == *pid) {
                                                                    disp.percentage = sysfs.percentage;
                                                                }
                                                            }
                                                            true
                                                        } else {
                                                            false
                                                        }
                                                    });
                                                }
                                            }
                                            Some(Err(err)) => {
                                                warn!(error = %err, "udev monitor stream error");
                                            }
                                            None => break,
                                        }
                                    }
                                }
                            }
                        })
                        .await;
                });
            })
            .ok()
    }
}

#[derive(Debug, Clone)]
pub enum BrightnessCmd {
    SetDisplay { id: String, percentage: u8 },
    SetAll { percentage: u8 },
}

/// System screen brightness service.
pub struct BrightnessService {
    tx: watch::Sender<BrightnessInfo>,
    rx: watch::Receiver<BrightnessInfo>,
    cmd_tx: Option<mpsc::UnboundedSender<BrightnessCmd>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    _udev_thread: Option<std::thread::JoinHandle<()>>,
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
            shutdown_tx: None,
            _udev_thread: None,
        }
    }

    pub fn new_with_sysfs_path(sysfs_base: &Path) -> Result<Self> {
        Self::new_with_adapters(
            sysfs_base,
            Arc::new(LogindDbusSetter::new()),
            SystemUdevMonitor,
        )
    }

    pub fn new_with_adapters<S: BrightnessSetter, M: UdevMonitor>(
        sysfs_base: &Path,
        setter: Arc<S>,
        monitor: M,
    ) -> Result<Self> {
        let (displays, primary_device, primary_sysfs) = Self::discover_initial_displays(sysfs_base);

        let available = !displays.is_empty();
        let primary_id = displays.first().map(|d| d.id.clone());
        let primary_pct = primary_sysfs
            .as_ref()
            .map(|s| s.percentage)
            .or_else(|| displays.first().map(|d| d.percentage))
            .unwrap_or(0);
        let primary_name = primary_device.as_ref().map(|d| d.name.clone());
        let max_raw = primary_sysfs.as_ref().map(|s| s.max_raw).unwrap_or(100);

        let initial_info = BrightnessInfo {
            percentage: primary_pct,
            available,
            device_name: primary_name.clone(),
            displays: displays.clone(),
            primary_display_id: primary_id,
            permissions_ok: true,
        };

        let (tx, rx) = watch::channel(initial_info);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<BrightnessCmd>();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let tx_ddc = tx.clone();
        tokio::task::spawn_blocking(move || {
            let (ddc_displays, ddc_perms) = discover_ddc_displays();
            if !ddc_displays.is_empty() || !ddc_perms {
                tx_ddc.send_if_modified(|curr| {
                    let mut modified = false;
                    if !ddc_displays.is_empty() {
                        curr.displays.extend(ddc_displays);
                        curr.available = true;
                        if curr.primary_display_id.is_none() {
                            curr.primary_display_id = curr.displays.first().map(|d| d.id.clone());
                            curr.percentage = curr.displays.first().map(|d| d.percentage).unwrap_or(0);
                        }
                        modified = true;
                    }
                    if !ddc_perms {
                        curr.permissions_ok = false;
                        modified = true;
                    }
                    modified
                });
            }
        });

        Self::spawn_coalesced_worker(cmd_rx, setter, displays, max_raw);

        let udev_thread = if let Some(ref device) = primary_device {
            monitor.listen(
                tx.clone(),
                device.sysfs_path.clone(),
                device.name.clone(),
                shutdown_rx,
            )
        } else {
            None
        };

        Ok(Self {
            tx,
            rx,
            cmd_tx: Some(cmd_tx),
            shutdown_tx: Some(shutdown_tx),
            _udev_thread: udev_thread,
        })
    }

    fn discover_initial_displays(
        sysfs_base: &Path,
    ) -> (Vec<DisplayBrightnessInfo>, Option<BacklightDevice>, Option<SysfsBrightness>) {
        let mut displays = Vec::new();
        if let Some(device) = discover_primary_backlight(sysfs_base) {
            if let Some(sysfs) = read_sysfs_brightness(&device.sysfs_path) {
                displays.push(DisplayBrightnessInfo {
                    id: format!("sysfs:{}", device.name),
                    name: format!("Internal Display ({})", device.name),
                    connector: Some("eDP-1".to_string()),
                    percentage: sysfs.percentage,
                    is_primary: true,
                    backend: BrightnessBackend::SysfsLogind {
                        sysfs_path: device.sysfs_path.clone(),
                        device_name: device.name.clone(),
                    },
                });
                return (displays, Some(device), Some(sysfs));
            }
        }
        (displays, None, None)
    }

    fn spawn_coalesced_worker<S: BrightnessSetter>(
        mut cmd_rx: mpsc::UnboundedReceiver<BrightnessCmd>,
        setter: Arc<S>,
        displays: Vec<DisplayBrightnessInfo>,
        primary_max_raw: u32,
    ) {
        tokio::spawn(async move {
            let mut pending: HashMap<String, u8> = HashMap::new();
            let display_map: HashMap<String, DisplayBrightnessInfo> = displays
                .into_iter()
                .map(|d| (d.id.clone(), d))
                .collect();

            let queue_cmd = |cmd: BrightnessCmd, pending: &mut HashMap<String, u8>| match cmd {
                BrightnessCmd::SetDisplay { id, percentage } => {
                    pending.insert(id, percentage.min(100));
                }
                BrightnessCmd::SetAll { percentage } => {
                    let pct = percentage.min(100);
                    for id in display_map.keys() {
                        pending.insert(id.clone(), pct);
                    }
                }
            };

            while let Some(cmd) = cmd_rx.recv().await {
                queue_cmd(cmd, &mut pending);

                while let Ok(cmd) = cmd_rx.try_recv() {
                    queue_cmd(cmd, &mut pending);
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

                for (id, target_pct) in pending.drain() {
                    if let Some(display) = display_map.get(&id) {
                        match &display.backend {
                            BrightnessBackend::SysfsLogind { device_name, .. } => {
                                let target_raw =
                                    ((target_pct as u64 * primary_max_raw as u64) as f64 / 100.0)
                                        .round() as u32;
                                let _ = setter.set_brightness(device_name, target_raw).await;
                            }
                            BrightnessBackend::DdcCiDirect { bus_index } => {
                                let bus = *bus_index;
                                let pct = target_pct;
                                tokio::task::spawn_blocking(move || {
                                    let dev_path = format!("/dev/i2c-{}", bus);
                                    if let Ok(mut dev) = from_i2c_device(dev_path) {
                                        let _ = dev.set_vcp_feature(
                                            VCP_LUMINANCE,
                                            pct as u16,
                                        );
                                    }
                                })
                                .await
                                .ok();
                            }
                        }
                    }
                }
            }
        });
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
        for disp in &mut current.displays {
            disp.percentage = percentage;
        }
        let _ = self.tx.send(current);

        if let Some(ref cmd_tx) = self.cmd_tx {
            let _ = cmd_tx.send(BrightnessCmd::SetAll { percentage });
        }
    }

    pub fn set_display_brightness(&self, display_id: &str, percentage: u8) {
        let percentage = percentage.min(100);
        let mut current = self.brightness_info();
        if let Some(disp) = current.displays.iter_mut().find(|d| d.id == display_id) {
            disp.percentage = percentage;
        }
        if current.primary_display_id.as_deref() == Some(display_id) {
            current.percentage = percentage;
        }
        let _ = self.tx.send(current);

        if let Some(ref cmd_tx) = self.cmd_tx {
            let _ = cmd_tx.send(BrightnessCmd::SetDisplay {
                id: display_id.to_string(),
                percentage,
            });
        }
    }

    pub fn adjust_display_brightness(&self, display_id: &str, delta: i8) {
        let current = self.brightness_info();
        if let Some(disp) = current.displays.iter().find(|d| d.id == display_id) {
            let new_pct = (disp.percentage as i16 + delta as i16).clamp(0, 100) as u8;
            self.set_display_brightness(display_id, new_pct);
        }
    }

    pub fn adjust_focused_brightness(&self, focused_connector: &str, delta: i8) {
        let current = self.brightness_info();
        if let Some(disp) = current
            .displays
            .iter()
            .find(|d| d.connector.as_deref() == Some(focused_connector))
        {
            let target_id = disp.id.clone();
            self.adjust_display_brightness(&target_id, delta);
        } else if let Some(ref primary_id) = current.primary_display_id {
            self.adjust_display_brightness(primary_id, delta);
        } else {
            let new_pct = (current.percentage as i16 + delta as i16).clamp(0, 100) as u8;
            self.set_brightness(new_pct);
        }
    }

    pub fn set_brightness_smooth(&self, target_percentage: u8) {
        let target = target_percentage.min(100);
        let log_target = BrightnessInfo::perceptual_percent_to_raw(target);
        self.set_brightness(log_target);
    }
}

impl Drop for BrightnessService {
    fn drop(&mut self) {
        if let Some(ref shutdown_tx) = self.shutdown_tx {
            let _ = shutdown_tx.send(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;

    #[derive(Default)]
    struct MockDbusSetter {
        calls: Arc<StdMutex<Vec<(String, u32)>>>,
    }

    impl BrightnessSetter for MockDbusSetter {
        async fn set_brightness(&self, device_name: &str, target_raw: u32) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push((device_name.to_string(), target_raw));
            Ok(())
        }
    }

    struct MockUdevMonitor {
        trigger_tx: mpsc::UnboundedSender<()>,
    }

    impl UdevMonitor for MockUdevMonitor {
        fn listen(
            &self,
            tx: watch::Sender<BrightnessInfo>,
            sysfs_path: PathBuf,
            device_name: String,
            mut shutdown_rx: watch::Receiver<bool>,
        ) -> Option<std::thread::JoinHandle<()>> {
            let (event_tx, mut event_rx) = mpsc::unbounded_channel::<()>();
            let trigger_tx = self.trigger_tx.clone();

            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                rt.block_on(async move {
                    loop {
                        tokio::select! {
                            _ = shutdown_rx.changed() => {
                                if *shutdown_rx.borrow() {
                                    break;
                                }
                            }
                            res = event_rx.recv() => {
                                if res.is_none() {
                                    break;
                                }
                                if let Some(sysfs) = read_sysfs_brightness(&sysfs_path) {
                                    let _ = tx.send(BrightnessInfo {
                                        percentage: sysfs.percentage,
                                        available: true,
                                        device_name: Some(device_name.clone()),
                                        displays: vec![],
                                        primary_display_id: None,
                                        permissions_ok: true,
                                    });
                                }
                            }
                        }
                    }
                });
            });

            let _ = (trigger_tx, event_tx);
            None
        }
    }

    #[test]
    fn default_is_unavailable() {
        assert_eq!(BrightnessInfo::default().percentage, 0);
        assert!(!BrightnessInfo::default().available);
        assert_eq!(BrightnessInfo::default().device_name, None);
        assert!(BrightnessInfo::default().displays.is_empty());
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

        let acpi_dir = sysfs_base.join("acpi_video0");
        std::fs::create_dir(&acpi_dir).unwrap();
        std::fs::write(acpi_dir.join("brightness"), "10\n").unwrap();
        std::fs::write(acpi_dir.join("max_brightness"), "100\n").unwrap();
        std::fs::write(acpi_dir.join("type"), "firmware\n").unwrap();

        let intel_dir = sysfs_base.join("intel_backlight");
        std::fs::create_dir(&intel_dir).unwrap();
        std::fs::write(intel_dir.join("brightness"), "150\n").unwrap();
        std::fs::write(intel_dir.join("max_brightness"), "255\n").unwrap();
        std::fs::write(intel_dir.join("type"), "raw\n").unwrap();

        let device = discover_primary_backlight(sysfs_base).expect("should find device");
        assert_eq!(device.name, "intel_backlight");
        assert_eq!(device.device_type, BacklightType::Raw);

        let sysfs = read_sysfs_brightness(&intel_dir).unwrap();
        assert_eq!(sysfs.current_raw, 150);
        assert_eq!(sysfs.max_raw, 255);
        assert_eq!(sysfs.percentage, 59);
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

        let setter = Arc::new(MockDbusSetter::default());
        let (trigger_tx, _trigger_rx) = mpsc::unbounded_channel();
        let monitor = MockUdevMonitor { trigger_tx };

        let service =
            BrightnessService::new_with_adapters(sysfs_base, setter.clone(), monitor).unwrap();
        let info = service.brightness_info();
        assert!(info.available);
        assert_eq!(info.percentage, 50);
        assert_eq!(info.device_name.as_deref(), Some("amdgpu_bl0"));
        assert!(!info.displays.is_empty());
        assert_eq!(info.displays[0].id, "sysfs:amdgpu_bl0");

        service.set_brightness(75);
        assert_eq!(service.brightness_info().percentage, 75);

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let calls = setter.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], ("amdgpu_bl0".to_string(), 75));
    }

    #[tokio::test]
    async fn test_multi_display_per_display_control() {
        let temp_dir = TempDir::new().unwrap();
        let sysfs_base = temp_dir.path();

        let intel_dir = sysfs_base.join("intel_backlight");
        std::fs::create_dir(&intel_dir).unwrap();
        std::fs::write(intel_dir.join("brightness"), "50\n").unwrap();
        std::fs::write(intel_dir.join("max_brightness"), "100\n").unwrap();
        std::fs::write(intel_dir.join("type"), "raw\n").unwrap();

        let setter = Arc::new(MockDbusSetter::default());
        let (trigger_tx, _trigger_rx) = mpsc::unbounded_channel();
        let monitor = MockUdevMonitor { trigger_tx };

        let service =
            BrightnessService::new_with_adapters(sysfs_base, setter.clone(), monitor).unwrap();

        service.set_display_brightness("sysfs:intel_backlight", 80);
        assert_eq!(service.brightness_info().percentage, 80);

        service.adjust_display_brightness("sysfs:intel_backlight", -10);
        assert_eq!(service.brightness_info().percentage, 70);

        service.adjust_focused_brightness("eDP-1", 5);
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
