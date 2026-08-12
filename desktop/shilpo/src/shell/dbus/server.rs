//! org.shilpo.Shell D-Bus service server implementation.

use super::types::{CommandResult, ShellStatus, ShellTelemetry};
use shilpo_services::CompositorCommandBroker;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use zbus::object_server::SignalEmitter;

/// Internal command payload sent to GPUI thread mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCommand {
    ReloadConfig,
    ShowBar,
    HideBar,
    ToggleBar,
    ShowOverview,
    HideOverview,
    ToggleOverview,
    SetBrightness(u8),
    SetDisplayBrightness { display_id: String, percentage: u8 },
    Capture(shilpo_services::capture::CaptureIntent),
}

/// D-Bus interface implementation for `org.shilpo.Shell`.
#[derive(Clone)]
pub struct ShellDbusService {
    mailbox_tx: mpsc::Sender<ShellCommand>,
    compositor_broker: Arc<Mutex<Option<Arc<CompositorCommandBroker>>>>,
    status: Arc<Mutex<ShellStatus>>,
    telemetry: Arc<Mutex<ShellTelemetry>>,
    last_workspace: Arc<Mutex<Option<(u64, u64, u64)>>>,
    last_theme: Arc<Mutex<Option<(String, String)>>>,
}

impl ShellDbusService {
    pub fn new(
        mailbox_tx: mpsc::Sender<ShellCommand>,
        compositor_broker: Arc<Mutex<Option<Arc<CompositorCommandBroker>>>>,
        status: Arc<Mutex<ShellStatus>>,
        telemetry: Arc<Mutex<ShellTelemetry>>,
    ) -> Self {
        Self {
            mailbox_tx,
            compositor_broker,
            status,
            telemetry,
            last_workspace: Arc::new(Mutex::new(None)),
            last_theme: Arc::new(Mutex::new(None)),
        }
    }

    fn send_command(&self, cmd: ShellCommand) -> zbus::fdo::Result<()> {
        match self.mailbox_tx.try_send(cmd) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(zbus::fdo::Error::LimitsExceeded(
                "command mailbox is full".into(),
            )),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(zbus::fdo::Error::Failed("shell daemon is stopping".into()))
            }
        }
    }

    fn execute_compositor_command(
        &self,
        cmd: shilpo_services::CompositorCommand,
    ) -> zbus::fdo::Result<CommandResult> {
        let broker = self
            .compositor_broker
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| {
                zbus::fdo::Error::Failed("compositor command broker is unavailable".into())
            })?;

        let outcome = match broker.submit(cmd) {
            Ok(ticket) => ticket.wait_timeout(std::time::Duration::from_secs(2)),
            Err(outcome) => outcome,
        };

        Ok(CommandResult::from(outcome))
    }

    pub fn update_status(&self, status: ShellStatus) {
        *self.status.lock().unwrap() = status;
    }

    pub fn update_telemetry(&self, telemetry: ShellTelemetry) {
        *self.telemetry.lock().unwrap() = telemetry;
    }

    pub async fn emit_workspace_changed_if_needed(
        &self,
        emitter: &SignalEmitter<'_>,
        workspace_id: u64,
        owner_generation: u64,
        revision: u64,
    ) {
        let key = (workspace_id, 0, 0);
        let should_emit = {
            let mut last = self.last_workspace.lock().unwrap();
            if *last != Some(key) {
                *last = Some(key);
                true
            } else {
                false
            }
        };
        if should_emit {
            let _ =
                Self::workspace_changed(emitter, workspace_id, owner_generation, revision).await;
        }
    }

    pub async fn emit_theme_changed_if_needed(
        &self,
        emitter: &SignalEmitter<'_>,
        mode: &str,
        scheme_variant: &str,
    ) {
        let key = (mode.to_string(), scheme_variant.to_string());
        let should_emit = {
            let mut last = self.last_theme.lock().unwrap();
            if let Some(prev) = last.as_ref() {
                if prev != &key {
                    *last = Some(key.clone());
                    true
                } else {
                    false
                }
            } else {
                *last = Some(key);
                false
            }
        };
        if should_emit {
            let _ = Self::theme_changed(emitter, mode, scheme_variant).await;
        }
    }

    pub async fn emit_config_reloaded(
        &self,
        emitter: &SignalEmitter<'_>,
        success: bool,
        mut changed_components: Vec<String>,
        diagnostic_count: u32,
    ) {
        if !success {
            changed_components.clear();
        } else {
            changed_components.sort();
        }
        let _ = Self::config_reloaded(emitter, success, changed_components, diagnostic_count).await;
    }
}

#[zbus::interface(name = "org.shilpo.Shell")]
impl ShellDbusService {
    async fn reload_config(&self) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "reload_config",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.send_command(ShellCommand::ReloadConfig)
    }

    async fn show_bar(&self) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "show_bar",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.send_command(ShellCommand::ShowBar)
    }

    async fn hide_bar(&self) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "hide_bar",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.send_command(ShellCommand::HideBar)
    }

    async fn toggle_bar(&self) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "toggle_bar",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.send_command(ShellCommand::ToggleBar)
    }

    async fn show_overview(&self) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "show_overview",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.send_command(ShellCommand::ShowOverview)
    }

    async fn hide_overview(&self) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "hide_overview",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.send_command(ShellCommand::HideOverview)
    }

    async fn toggle_overview(&self) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "toggle_overview",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.send_command(ShellCommand::ToggleOverview)
    }

    async fn focus_workspace(&self, workspace_id: u64) -> zbus::fdo::Result<CommandResult> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "focus_workspace",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.execute_compositor_command(shilpo_services::CompositorCommand::FocusWorkspace(
            workspace_id,
        ))
    }

    async fn create_workspace(&self) -> zbus::fdo::Result<CommandResult> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "create_workspace",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.execute_compositor_command(shilpo_services::CompositorCommand::CreateWorkspace)
    }

    async fn focus_window(&self, window_id: u64) -> zbus::fdo::Result<CommandResult> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "focus_window",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.execute_compositor_command(shilpo_services::CompositorCommand::FocusWindow(window_id))
    }

    async fn focus_previous_window(&self) -> zbus::fdo::Result<CommandResult> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "focus_previous_window",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.execute_compositor_command(shilpo_services::CompositorCommand::FocusPreviousWindow)
    }

    async fn close_window(&self, window_id: u64) -> zbus::fdo::Result<CommandResult> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "close_window",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.execute_compositor_command(shilpo_services::CompositorCommand::CloseWindow(window_id))
    }

    async fn move_window_to_workspace(
        &self,
        window_id: u64,
        workspace_id: u64,
    ) -> zbus::fdo::Result<CommandResult> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "move_window_to_workspace",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.execute_compositor_command(shilpo_services::CompositorCommand::MoveWindowToWorkspace {
            window_id,
            workspace_id,
        })
    }

    async fn set_brightness(&self, percentage: u8) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "set_brightness",
            outcome = "success"
        );
        let _enter = _span.enter();
        if percentage > 100 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "brightness percentage must be between 0 and 100".into(),
            ));
        }
        self.send_command(ShellCommand::SetBrightness(percentage))
    }

    async fn set_display_brightness(
        &self,
        display_id: String,
        percentage: u8,
    ) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "set_display_brightness",
            outcome = "success"
        );
        let _enter = _span.enter();
        if display_id.trim().is_empty() {
            return Err(zbus::fdo::Error::InvalidArgs(
                "display_id cannot be empty".into(),
            ));
        }
        if percentage > 100 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "brightness percentage must be between 0 and 100".into(),
            ));
        }
        self.send_command(ShellCommand::SetDisplayBrightness {
            display_id,
            percentage,
        })
    }

    async fn get_status(&self) -> ShellStatus {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "get_status",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.status.lock().unwrap().clone()
    }

    async fn get_telemetry(&self) -> ShellTelemetry {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "get_telemetry",
            outcome = "success"
        );
        let _enter = _span.enter();
        self.telemetry.lock().unwrap().clone()
    }

    async fn capture(&self, intent: String) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Shell",
            operation = "capture",
            outcome = "success"
        );
        let _enter = _span.enter();
        let capture_intent = match intent.as_str() {
            "clipboard" => shilpo_services::capture::CaptureIntent::Clipboard,
            "annotation" => shilpo_services::capture::CaptureIntent::Annotation,
            "ocr" => shilpo_services::capture::CaptureIntent::Ocr,
            "menu" => shilpo_services::capture::CaptureIntent::Menu,
            _ => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "unknown capture intent '{intent}'"
                )));
            }
        };
        self.send_command(ShellCommand::Capture(capture_intent))
    }

    #[zbus(signal)]
    pub async fn shell_started(
        signal_ctor: &SignalEmitter<'_>,
        instance_id: &str,
        pid: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn shell_stopping(
        signal_ctor: &SignalEmitter<'_>,
        instance_id: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn workspace_changed(
        signal_ctor: &SignalEmitter<'_>,
        workspace_id: u64,
        owner_generation: u64,
        revision: u64,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn theme_changed(
        signal_ctor: &SignalEmitter<'_>,
        mode: &str,
        scheme_variant: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn config_reloaded(
        signal_ctor: &SignalEmitter<'_>,
        success: bool,
        changed_components: Vec<String>,
        diagnostic_count: u32,
    ) -> zbus::Result<()>;
}
