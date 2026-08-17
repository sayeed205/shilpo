//! Systemd adapter for managing `shilpo-shell.service` user unit and polling D-Bus status.

use super::ipc::IpcAdapter;
use crate::cli::output::{EXIT_FAILURE, EXIT_UNAVAILABLE};
use crate::shell::dbus::ShellStatus;
use std::process::Command;
use std::time::{Duration, Instant};

pub const SERVICE_NAME: &str = "shilpo-shell.service";

pub struct SystemdAdapter {
    ipc: IpcAdapter,
}

impl Default for SystemdAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemdAdapter {
    pub fn new() -> Self {
        Self {
            ipc: IpcAdapter::new(),
        }
    }

    pub fn is_unit_installed() -> bool {
        let output = Command::new("systemctl")
            .args(["--user", "list-unit-files", SERVICE_NAME])
            .output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.contains(SERVICE_NAME)
            }
            Err(_) => false,
        }
    }

    pub fn is_unit_active() -> bool {
        let output = Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", SERVICE_NAME])
            .status();
        match output {
            Ok(status) => status.success(),
            Err(_) => false,
        }
    }

    pub fn start(&self, timeout: Duration) -> Result<ShellStatus, (i32, String)> {
        if !Self::is_unit_installed() {
            return Err((
                3,
                format!(
                    "systemd unit '{SERVICE_NAME}' not found.\nPlease install {SERVICE_NAME} to ~/.config/systemd/user/ or /usr/lib/systemd/user/ and run 'systemctl --user daemon-reload'."
                ),
            ));
        }

        let status = Command::new("systemctl")
            .args(["--user", "start", SERVICE_NAME])
            .status();
        if status.is_err() || !status.unwrap().success() {
            return Err((
                3,
                format!("failed to start systemd user unit '{SERVICE_NAME}'"),
            ));
        }

        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(status) = self.ipc.status()
                && matches!(status.readiness.as_str(), "ready" | "degraded")
            {
                return Ok(status);
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        Err((
            4,
            format!(
                "timed out after {:?} waiting for shell daemon readiness",
                timeout
            ),
        ))
    }

    pub fn stop(&self, timeout: Duration) -> Result<(), (i32, String)> {
        let status = Command::new("systemctl")
            .args(["--user", "stop", SERVICE_NAME])
            .status();
        if status.is_err() || !status.unwrap().success() {
            return Err((
                3,
                format!("failed to stop systemd user unit '{SERVICE_NAME}'"),
            ));
        }

        let start = Instant::now();
        while start.elapsed() < timeout {
            let active = Self::is_unit_active();
            let daemon_gone = match self.ipc.status() {
                Err((code, _)) => code == 3,
                Ok(_) => false,
            };
            if !active && daemon_gone {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        Err((
            4,
            format!(
                "timed out after {:?} waiting for shell daemon to stop",
                timeout
            ),
        ))
    }

    pub fn restart(&self, timeout: Duration) -> Result<ShellStatus, (i32, String)> {
        if !Self::is_unit_installed() {
            return Err((
                3,
                format!(
                    "systemd unit '{SERVICE_NAME}' not found.\nPlease install {SERVICE_NAME} to ~/.config/systemd/user/ or /usr/lib/systemd/user/ and run 'systemctl --user daemon-reload'."
                ),
            ));
        }

        let old_instance_id = self.ipc.status().ok().map(|s| s.instance_id);

        let status = Command::new("systemctl")
            .args(["--user", "restart", SERVICE_NAME])
            .status();
        if status.is_err() || !status.unwrap().success() {
            return Err((
                3,
                format!("failed to restart systemd user unit '{SERVICE_NAME}'"),
            ));
        }

        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(status) = self.ipc.status() {
                let is_new_instance = match &old_instance_id {
                    Some(old_id) => !status.instance_id.is_empty() && status.instance_id != *old_id,
                    None => true,
                };
                let is_ready = matches!(status.readiness.as_str(), "ready" | "degraded");
                if is_new_instance && is_ready {
                    return Ok(status);
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        Err((
            4,
            format!(
                "timed out after {:?} waiting for new shell daemon instance after restart",
                timeout
            ),
        ))
    }

    pub fn logs(
        &self,
        follow: bool,
        since: Option<&str>,
        lines: Option<usize>,
    ) -> Result<i32, (i32, String)> {
        let mut cmd = Command::new("journalctl");
        cmd.args(["--user", "-u", SERVICE_NAME]);
        if follow {
            cmd.arg("-f");
        }
        if let Some(s) = since {
            cmd.arg("--since").arg(s);
        }
        if let Some(n) = lines {
            cmd.arg("-n").arg(n.to_string());
        }

        let status = cmd.status().map_err(|e| {
            (
                3,
                format!("failed to execute journalctl for unit '{SERVICE_NAME}': {e}"),
            )
        })?;

        Ok(status.code().unwrap_or(0))
    }

    pub fn logs_capture(
        &self,
        since: Option<&str>,
        lines: Option<usize>,
    ) -> Result<String, (i32, String)> {
        let mut cmd = Command::new("journalctl");
        cmd.args(["--user", "-u", SERVICE_NAME, "--no-pager"]);
        if let Some(since) = since {
            cmd.arg("--since").arg(since);
        }
        if let Some(lines) = lines {
            cmd.arg("-n").arg(lines.to_string());
        }
        let output = cmd.output().map_err(|error| {
            (
                EXIT_UNAVAILABLE,
                format!("failed to execute journalctl: {error}"),
            )
        })?;
        if !output.status.success() {
            return Err((
                EXIT_FAILURE,
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
