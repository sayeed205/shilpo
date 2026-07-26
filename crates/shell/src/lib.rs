pub mod actions;
pub mod bar;
pub mod control_center;
pub mod doctor;
pub mod error;
pub mod extension_surface;
pub mod extensions;
pub mod launcher;
pub mod notification;
pub mod osd;
pub mod overview;
pub mod runtime;

pub use actions::{
    ActionCategory, ActionDescriptor, ActionId, ActionRegistry, KeybindingManager, Shortcut,
};
pub use bar::BarView;
pub use control_center::ControlCenterView;
pub use doctor::DoctorChecker;
pub use error::ShellError;
pub use extensions::{
    ContributionDescriptor, ContributionInstance, ContributionSurface, ShellExtensions,
};
pub use launcher::LauncherView;
pub use notification::NotificationToastView;
pub use osd::{OsdKind, OsdView};
pub use overview::WorkspaceOverview;
pub use runtime::ShellRuntime;
