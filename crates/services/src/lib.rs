pub mod applications;
pub mod audio;
pub mod brightness;
pub mod compositor;
pub mod error;
pub mod ipc;
pub mod network;
pub mod night_light;
pub mod notifications;
pub mod tray;
pub mod upower;

pub use applications::{AppScanner, Application};
pub use audio::{AudioInfo, AudioService};
pub use brightness::{BrightnessInfo, BrightnessService};
pub use compositor::{
    CompositorAdapter, CompositorCapabilities, NiriCompositorService, NiriWorkspaceInfo,
    WindowInfo, WorkspaceInfo,
};
pub use error::ServiceError;
pub use ipc::{BarState, IpcError, IpcRequest, IpcResponse, IpcResult, IpcStatus, ShellIpcServer};
pub use network::{NetworkInfo, NetworkService};
pub use night_light::{NightLightInfo, NightLightService};
pub use notifications::{Notification, NotificationService};
pub use tray::{TrayItem, TrayService};
pub use upower::{BatteryInfo, BatteryService};
