pub mod applications;
pub mod audio;
pub mod bluetooth;
pub mod brightness;
pub mod clipboard;
pub mod compositor;
pub mod error;
pub mod ipc;
pub mod location;
pub mod network;
pub mod night_light;
pub mod notifications;
pub mod palette;
pub mod power_profile;
pub mod screen_capture;
pub mod tray;
pub mod upower;

pub use applications::{
    AppScanner, Application, parse_uri_list, percent_decode, resolve_handler_for_uri,
    validate_drag_drop_payload,
};
pub use audio::{AudioDevice, AudioInfo, AudioPort, AudioService, AudioStream};
pub use bluetooth::{BluetoothInfo, BluetoothService};
pub use brightness::{BrightnessInfo, BrightnessService};
pub use clipboard::ClipboardService;
pub use compositor::{
    CompositorAdapter, CompositorCapabilities, CompositorCapability, NiriCompositorService,
    NiriWorkspaceInfo, WindowInfo, WorkspaceInfo,
};
pub use error::ServiceError;
pub use ipc::{
    BarState, IpcError, IpcRequest, IpcResponse, IpcResult, IpcStatus, ServiceHealth,
    ShellIpcServer,
};
pub use location::{LocationInfo, LocationService};
pub use network::{NetworkInfo, NetworkService, VpnConnection};
pub use night_light::{NightLightInfo, NightLightService, ThemeSchedule, should_use_dark_mode};
pub use notifications::{Notification, NotificationService, NotificationUrgency};
pub use palette::PaletteExtractor;
pub use power_profile::{PowerProfile, PowerProfileInfo, PowerProfileService};
pub use screen_capture::{RecordMode, ScreenCaptureInfo, ScreenCaptureService, ScreenshotMode};
pub use tray::{TrayItem, TrayMenuItem, TrayService};
pub use upower::{BatteryInfo, BatteryService};

/// Returns whether Niri Wayland compositor IPC is supported on this target platform.
pub fn is_niri_supported() -> bool {
    cfg!(target_os = "linux")
}

/// Returns the supported system service capability matrix on this platform.
pub fn platform_capabilities() -> Vec<&'static str> {
    if cfg!(target_os = "linux") {
        vec![
            "compositor",
            "brightness",
            "audio",
            "notifications",
            "tray",
            "network",
            "upower",
            "location",
        ]
    } else {
        vec!["offline_fallback"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_capability_matrix() {
        assert!(is_niri_supported());
        let caps = platform_capabilities();
        assert!(caps.contains(&"compositor"));
    }
}
