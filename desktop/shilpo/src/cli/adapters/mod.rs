pub mod doctor;
pub mod ext;
pub mod ipc;
pub mod migrate;
pub mod systemd;
pub mod theme;

pub use doctor::DoctorChecker;
pub use ext::ExtAdapter;
pub use ipc::IpcAdapter;
pub use migrate::ConfigMigrateAdapter;
pub use systemd::SystemdAdapter;
pub use theme::ThemeAdapter;
