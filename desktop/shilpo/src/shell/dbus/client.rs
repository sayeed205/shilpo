//! D-Bus proxy client for org.shilpo.Shell.

use super::types::{CommandResult, ShellStatus, ShellTelemetry};

#[zbus::proxy(
    interface = "org.shilpo.Shell",
    default_service = "org.shilpo.Shell",
    default_path = "/org/shilpo/Shell"
)]
pub trait Shell {
    async fn reload_config(&self) -> zbus::Result<()>;
    async fn show_bar(&self) -> zbus::Result<()>;
    async fn hide_bar(&self) -> zbus::Result<()>;
    async fn toggle_bar(&self) -> zbus::Result<()>;
    async fn show_overview(&self) -> zbus::Result<()>;
    async fn hide_overview(&self) -> zbus::Result<()>;
    async fn toggle_overview(&self) -> zbus::Result<()>;
    async fn focus_workspace(&self, workspace_id: u64) -> zbus::Result<CommandResult>;
    async fn create_workspace(&self) -> zbus::Result<CommandResult>;
    async fn focus_window(&self, window_id: u64) -> zbus::Result<CommandResult>;
    async fn focus_previous_window(&self) -> zbus::Result<CommandResult>;
    async fn close_window(&self, window_id: u64) -> zbus::Result<CommandResult>;
    async fn move_window_to_workspace(
        &self,
        window_id: u64,
        workspace_id: u64,
    ) -> zbus::Result<CommandResult>;
    async fn set_brightness(&self, percentage: u8) -> zbus::Result<()>;
    async fn set_display_brightness(&self, display_id: String, percentage: u8) -> zbus::Result<()>;
    async fn get_status(&self) -> zbus::Result<ShellStatus>;
    async fn get_telemetry(&self) -> zbus::Result<ShellTelemetry>;
    async fn capture(&self, intent: String) -> zbus::Result<()>;
    async fn invoke_action(
        &self,
        action_id: String,
        payload_json: Option<String>,
    ) -> zbus::Result<()>;
    async fn next_wallpaper(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn shell_started(&self, instance_id: String, pid: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn shell_stopping(&self, instance_id: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn workspace_changed(
        &self,
        workspace_id: u64,
        owner_generation: u64,
        revision: u64,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn theme_changed(&self, mode: String, scheme_variant: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn config_reloaded(
        &self,
        success: bool,
        changed_components: Vec<String>,
        diagnostic_count: u32,
    ) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.shilpo.Debug",
    default_service = "org.shilpo.Shell",
    default_path = "/org/shilpo/Shell"
)]
pub trait Debug {
    async fn set_log_filter(&self, filter: String) -> zbus::Result<()>;
    async fn get_log_filter(&self) -> zbus::Result<String>;
    async fn emit_test_notification(&self, title: String, body: String) -> zbus::Result<()>;
}
