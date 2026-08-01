pub mod actions;
mod app_icons;
pub mod bar;
mod battery;
pub mod control_center;
pub mod error;
mod extension_http;
pub mod extension_surface;
pub mod extensions;
pub mod notification;
pub mod osd;
pub mod overview;
pub mod overview_search;
pub mod runtime;

pub use actions::{
    ActionCategory, ActionDescriptor, ActionId, ActionRegistry, KeybindingManager, Shortcut,
};
pub use bar::BarView;
pub use control_center::ControlCenterView;
pub use error::ShellError;
pub use extensions::{
    ContributionDescriptor, ContributionInstance, ContributionSurface, ExtensionCoordinator,
    ExtensionSnapshot,
};
pub use notification::NotificationToastView;
pub use osd::{OsdKind, OsdView};
pub use overview::WorkspaceOverview;
pub use runtime::ShellRuntime;
