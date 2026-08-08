mod adapters;
mod args;
mod output;
#[cfg(test)]
mod tests;

use adapters::{DoctorChecker, ExtAdapter, IpcAdapter, SystemdAdapter, ThemeAdapter};
use args::{
    BrightnessCommands, CaptureAction, Cli, Commands, ConfigCommands, ExtCommands, ModeValue,
    RecordAction, ShellCommands, ThemeCommands, ThemeModeAction, ThemeSeedAction,
    ThemeWallpaperAction, VisibilityAction, WindowCommands, WorkspaceCommands,
};
use clap::{CommandFactory, Parser};
use output::{CliOutput, EXIT_FAILURE, EXIT_INVALID_ARGS, EXIT_SUCCESS};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let raw_args: Vec<String> = std::env::args().collect();
    if raw_args.len() <= 1 {
        let mut cmd = Cli::command();
        cmd.print_help().unwrap();
        println!();
        std::process::exit(EXIT_SUCCESS);
    }

    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let exit_code = if e.use_stderr() {
                EXIT_INVALID_ARGS
            } else {
                EXIT_SUCCESS
            };
            if raw_args.iter().any(|arg| arg == "--json") {
                println!(
                    "{}",
                    serde_json::json!({
                        "schema_version": 1,
                        "ok": false,
                        "command": "usage",
                        "data": null,
                        "warnings": [],
                        "error": { "code": "usage.invalid_arguments", "message": e.to_string() }
                    })
                );
            } else {
                print!("{e}");
            }
            std::process::exit(exit_code);
        }
    };

    let output = match CliOutput::new(cli.json, cli.quiet) {
        Ok(out) => out,
        Err((code, msg)) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "schema_version": 1,
                        "ok": false,
                        "command": "usage",
                        "data": null,
                        "warnings": [],
                        "error": { "code": "usage.invalid_options", "message": msg }
                    })
                );
            } else {
                eprintln!("{msg}");
            }
            std::process::exit(code);
        }
    };

    let timeout = match parse_duration(cli.timeout.as_deref()) {
        Ok(timeout) => timeout,
        Err(message) => {
            let code = output.error(
                "usage",
                "usage.invalid_timeout",
                &message,
                None,
                Vec::new(),
                EXIT_INVALID_ARGS,
            );
            std::process::exit(code);
        }
    };

    let Some(command) = cli.command else {
        let mut cmd = Cli::command();
        cmd.print_help().unwrap();
        println!();
        std::process::exit(EXIT_SUCCESS);
    };

    let exit_code = match command {
        Commands::Shell { command } => match command {
            ShellCommands::Status => {
                let ipc = IpcAdapter::new();
                match ipc.status() {
                    Ok(status)
                        if matches!(status.readiness, shilpo_services::ReadinessState::Ready) =>
                    {
                        let unit_active = SystemdAdapter::is_unit_active();
                        let data = serde_json::json!({ "unit_active": unit_active, "ipc": status });
                        output.success(
                                "shell.status", &data,
                                Some(&format!("Systemd active: {unit_active}\nInstance ID: {}\nPID: {}\nReadiness: {:?}\nBar State: {:?}\nOverview Visible: {}\nControl Center Visible: {}", status.instance_id, status.pid, status.readiness, status.bar, status.overview_visible, status.control_center_visible)),
                                Vec::new(),
                            )
                    }
                    Ok(status) => output.error(
                        "shell.status",
                        "shell_degraded",
                        &format!("Shell readiness is {:?}", status.readiness),
                        Some(serde_json::to_value(&status).unwrap_or_default()),
                        Vec::new(),
                        EXIT_FAILURE,
                    ),
                    Err((code, msg)) => output.error(
                        "shell.status",
                        "daemon_unavailable",
                        &msg,
                        None,
                        Vec::new(),
                        code,
                    ),
                }
            }
            ShellCommands::Start => {
                let systemd = SystemdAdapter::new();
                match systemd.start(timeout) {
                    Ok(status)
                        if matches!(status.readiness, shilpo_services::ReadinessState::Ready) =>
                    {
                        output.success(
                            "shell.start",
                            &status,
                            Some("Shell daemon started successfully"),
                            Vec::new(),
                        )
                    }
                    Ok(status) => output.error(
                        "shell.start",
                        "shell_degraded",
                        &format!("Shell started with readiness {:?}", status.readiness),
                        Some(serde_json::to_value(&status).unwrap_or_default()),
                        Vec::new(),
                        EXIT_FAILURE,
                    ),
                    Err((code, msg)) => {
                        output.error("shell.start", "start_failed", &msg, None, Vec::new(), code)
                    }
                }
            }
            ShellCommands::Stop => {
                let systemd = SystemdAdapter::new();
                match systemd.stop(timeout) {
                    Ok(()) => output.success(
                        "shell.stop",
                        &serde_json::json!({ "stopped": true }),
                        Some("Shell daemon stopped"),
                        Vec::new(),
                    ),
                    Err((code, msg)) => {
                        output.error("shell.stop", "stop_failed", &msg, None, Vec::new(), code)
                    }
                }
            }
            ShellCommands::Restart => {
                let systemd = SystemdAdapter::new();
                match systemd.restart(timeout) {
                    Ok(status)
                        if matches!(status.readiness, shilpo_services::ReadinessState::Ready) =>
                    {
                        output.success(
                            "shell.restart",
                            &status,
                            Some("Shell daemon restarted successfully"),
                            Vec::new(),
                        )
                    }
                    Ok(status) => output.error(
                        "shell.restart",
                        "shell_degraded",
                        &format!("Shell restarted with readiness {:?}", status.readiness),
                        Some(serde_json::to_value(&status).unwrap_or_default()),
                        Vec::new(),
                        EXIT_FAILURE,
                    ),
                    Err((code, msg)) => output.error(
                        "shell.restart",
                        "restart_failed",
                        &msg,
                        None,
                        Vec::new(),
                        code,
                    ),
                }
            }
            ShellCommands::Logs {
                follow,
                since,
                lines,
            } => {
                let systemd = SystemdAdapter::new();
                if cli.json && follow {
                    output.error(
                        "shell.logs",
                        "json_stream_unsupported",
                        "--json cannot be combined with --follow",
                        None,
                        Vec::new(),
                        EXIT_INVALID_ARGS,
                    )
                } else if cli.json {
                    match systemd.logs_capture(since.as_deref(), lines) {
                        Ok(logs) => output.success(
                            "shell.logs",
                            &serde_json::json!({ "logs": logs }),
                            None,
                            Vec::new(),
                        ),
                        Err((code, msg)) => {
                            output.error("shell.logs", "logs_failed", &msg, None, Vec::new(), code)
                        }
                    }
                } else {
                    match systemd.logs(follow, since.as_deref(), lines) {
                        Ok(code) => code,
                        Err((code, msg)) => {
                            output.error("shell.logs", "logs_failed", &msg, None, Vec::new(), code)
                        }
                    }
                }
            }
            ShellCommands::Telemetry => {
                let ipc = IpcAdapter::new();
                match ipc.telemetry() {
                    Ok(health) => output.success("shell.telemetry", &health, Some(&format!("Service Health & Broker Telemetry:\n  Compositor Connected: {}\n  Compositor State: {}\n  Compositor Revision: {}\n  Uptime: {}s", health.compositor_connected, health.compositor_state, health.compositor_revision, health.uptime_seconds)), Vec::new()),
                    Err((code, msg)) => output.error("shell.telemetry", "telemetry_failed", &msg, None, Vec::new(), code),
                }
            }
        },
        Commands::Overview { action } => {
            let ipc = IpcAdapter::new();
            let res = match action {
                VisibilityAction::Show => ipc.overview_show(),
                VisibilityAction::Hide => ipc.overview_hide(),
                VisibilityAction::Toggle => ipc.overview_toggle(),
            };
            match res {
                Ok(()) => output.success(
                    "overview",
                    &serde_json::json!({ "action": format!("{action:?}") }),
                    Some(&format!("Overview action {action:?} applied")),
                    Vec::new(),
                ),
                Err((code, msg)) => {
                    output.error("overview", "ipc_failed", &msg, None, Vec::new(), code)
                }
            }
        }
        Commands::ControlCenter { action } => {
            let ipc = IpcAdapter::new();
            let res = match action {
                VisibilityAction::Show => ipc.control_center_show(),
                VisibilityAction::Hide => ipc.control_center_hide(),
                VisibilityAction::Toggle => ipc.control_center_toggle(),
            };
            match res {
                Ok(()) => output.success(
                    "control_center",
                    &serde_json::json!({ "action": format!("{action:?}") }),
                    Some(&format!("Control center action {action:?} applied")),
                    Vec::new(),
                ),
                Err((code, msg)) => {
                    output.error("control_center", "ipc_failed", &msg, None, Vec::new(), code)
                }
            }
        }
        Commands::Bar { action } => {
            let ipc = IpcAdapter::new();
            let res = match action {
                VisibilityAction::Show => ipc.bar_show(),
                VisibilityAction::Hide => ipc.bar_hide(),
                VisibilityAction::Toggle => ipc.bar_toggle(),
            };
            match res {
                Ok(()) => output.success(
                    "bar",
                    &serde_json::json!({ "action": format!("{action:?}") }),
                    Some(&format!("Bar action {action:?} applied")),
                    Vec::new(),
                ),
                Err((code, msg)) => output.error("bar", "ipc_failed", &msg, None, Vec::new(), code),
            }
        }
        Commands::Workspace { command } => {
            let ipc = IpcAdapter::new();
            let (cmd_name, res) = match command {
                WorkspaceCommands::Focus { id } => ("workspace.focus", ipc.workspace_focus(id)),
                WorkspaceCommands::Create => ("workspace.create", ipc.workspace_create()),
            };
            match res {
                Ok(()) => output.success(
                    cmd_name,
                    &serde_json::json!({ "ok": true }),
                    Some("Workspace command applied"),
                    Vec::new(),
                ),
                Err((code, msg)) => {
                    output.error(cmd_name, "ipc_failed", &msg, None, Vec::new(), code)
                }
            }
        }
        Commands::Window { command } => {
            let ipc = IpcAdapter::new();
            let (cmd_name, res) = match command {
                WindowCommands::Focus { id } => ("window.focus", ipc.window_focus(id)),
                WindowCommands::FocusPrevious => {
                    ("window.focus_previous", ipc.window_focus_previous())
                }
                WindowCommands::Move { id, workspace } => {
                    ("window.move", ipc.window_move(id, workspace))
                }
            };
            match res {
                Ok(()) => output.success(
                    cmd_name,
                    &serde_json::json!({ "ok": true }),
                    Some("Window command applied"),
                    Vec::new(),
                ),
                Err((code, msg)) => {
                    output.error(cmd_name, "ipc_failed", &msg, None, Vec::new(), code)
                }
            }
        }
        Commands::Config { command } => match command {
            ConfigCommands::Path => {
                let path = DoctorChecker::default_config_path();
                output.success(
                    "config.path",
                    &serde_json::json!({ "path": path }),
                    Some(&path.display().to_string()),
                    Vec::new(),
                )
            }
            ConfigCommands::Validate => {
                let path = DoctorChecker::default_config_path();
                match shilpo_config::ShellConfig::load(&path) {
                    Ok(_) => output.success(
                        "config.validate",
                        &serde_json::json!({ "valid": true, "path": path }),
                        Some(&format!("Configuration at {} is valid", path.display())),
                        Vec::new(),
                    ),
                    Err(e) => output.error(
                        "config.validate",
                        "invalid_config",
                        &format!("Configuration syntax error: {e}"),
                        None,
                        Vec::new(),
                        EXIT_FAILURE,
                    ),
                }
            }
            ConfigCommands::Reload => {
                let ipc = IpcAdapter::new();
                match ipc.config_reload() {
                    Ok(()) => output.success(
                        "config.reload",
                        &serde_json::json!({ "reloaded": true }),
                        Some("Config reload signal sent to shell daemon"),
                        Vec::new(),
                    ),
                    Err((code, msg)) => {
                        output.error("config.reload", "ipc_failed", &msg, None, Vec::new(), code)
                    }
                }
            }
        },
        Commands::Theme { command } => match command {
            ThemeCommands::Mode { action } => match action {
                ThemeModeAction::Get => match ThemeAdapter::get_mode().await {
                    Ok(mode) => output.success(
                        "theme.mode.get",
                        &serde_json::json!({ "mode": mode }),
                        Some(&mode),
                        Vec::new(),
                    ),
                    Err((code, msg)) => output.error(
                        "theme.mode.get",
                        "theme_daemon_unavailable",
                        &msg,
                        None,
                        Vec::new(),
                        code,
                    ),
                },
                ThemeModeAction::Set { mode } => {
                    let theme_mode = match mode {
                        ModeValue::Light => shilpo_theme::ThemeMode::Light,
                        ModeValue::Dark => shilpo_theme::ThemeMode::Dark,
                        ModeValue::System => shilpo_theme::ThemeMode::System,
                    };
                    match ThemeAdapter::set_mode(theme_mode).await {
                        Ok(msg) => output.success(
                            "theme.mode.set",
                            &serde_json::json!({ "mode": format!("{mode:?}") }),
                            Some(&msg),
                            Vec::new(),
                        ),
                        Err((code, msg)) => output.error(
                            "theme.mode.set",
                            "theme_daemon_unavailable",
                            &msg,
                            None,
                            Vec::new(),
                            code,
                        ),
                    }
                }
                ThemeModeAction::Toggle => match ThemeAdapter::toggle_mode().await {
                    Ok(msg) => output.success(
                        "theme.mode.toggle",
                        &serde_json::json!({ "toggled": true }),
                        Some(&msg),
                        Vec::new(),
                    ),
                    Err((code, msg)) => output.error(
                        "theme.mode.toggle",
                        "theme_daemon_unavailable",
                        &msg,
                        None,
                        Vec::new(),
                        code,
                    ),
                },
            },
            ThemeCommands::Seed { action } => match action {
                ThemeSeedAction::Set { color } => match ThemeAdapter::set_seed(&color).await {
                    Ok(msg) => output.success(
                        "theme.seed.set",
                        &serde_json::json!({ "seed": color }),
                        Some(&msg),
                        Vec::new(),
                    ),
                    Err((code, msg)) => output.error(
                        "theme.seed.set",
                        "theme_daemon_unavailable",
                        &msg,
                        None,
                        Vec::new(),
                        code,
                    ),
                },
            },
            ThemeCommands::Wallpaper { action } => match action {
                ThemeWallpaperAction::Get => match ThemeAdapter::get_wallpaper().await {
                    Ok(state) => {
                        let path = state.wallpaper_path.as_deref().map_or_else(
                            || "<none>".to_string(),
                            |path| path.display().to_string(),
                        );
                        let directory = state.wallpaper_dir.display().to_string();
                        output.success(
                            "theme.wallpaper.get",
                            &serde_json::json!({
                                "path": state.wallpaper_path,
                                "directory": state.wallpaper_dir,
                            }),
                            Some(&format!(
                                "Current wallpaper: {path}\nWallpaper directory: {directory}"
                            )),
                            Vec::new(),
                        )
                    }
                    Err((code, msg)) => output.error(
                        "theme.wallpaper.get",
                        "theme_daemon_unavailable",
                        &msg,
                        None,
                        Vec::new(),
                        code,
                    ),
                },
                ThemeWallpaperAction::Set { path } => {
                    match ThemeAdapter::set_wallpaper(&path).await {
                        Ok(msg) => output.success(
                            "theme.wallpaper.set",
                            &serde_json::json!({ "path": path }),
                            Some(&msg),
                            Vec::new(),
                        ),
                        Err((code, msg)) => output.error(
                            "theme.wallpaper.set",
                            "theme_daemon_unavailable",
                            &msg,
                            None,
                            Vec::new(),
                            code,
                        ),
                    }
                }
                ThemeWallpaperAction::Random => match ThemeAdapter::random_wallpaper().await {
                    Ok(msg) => output.success(
                        "theme.wallpaper.random",
                        &serde_json::json!({ "random": true }),
                        Some(&msg),
                        Vec::new(),
                    ),
                    Err((code, msg)) => output.error(
                        "theme.wallpaper.random",
                        "theme_daemon_unavailable",
                        &msg,
                        None,
                        Vec::new(),
                        code,
                    ),
                },
            },
        },
        Commands::Brightness { command } => match command {
            BrightnessCommands::List => {
                let (ddc_displays, ddc_perms) =
                    shilpo_services::brightness::discover_ddc_displays();
                let data = serde_json::json!({
                    "displays": ddc_displays,
                    "permissions_ok": ddc_perms,
                });
                let mut report = format!("Discovered DDC/CI Displays (Permissions OK: {ddc_perms}):\n");
                for d in &ddc_displays {
                    report.push_str(&format!(
                        " - [{}] {} (Connector: {}, Brightness: {}%)\n",
                        d.id,
                        d.name,
                        d.connector.as_deref().unwrap_or("<unknown>"),
                        d.percentage
                    ));
                }
                output.success("brightness.list", &data, Some(&report), Vec::new())
            }
            BrightnessCommands::Set { display, value } => {
                let ipc = IpcAdapter::new();
                let req = if let Some(disp_id) = display {
                    shilpo_services::IpcRequest::SetDisplayBrightness {
                        id: disp_id,
                        percentage: value,
                    }
                } else {
                    shilpo_services::IpcRequest::SetBrightness(value)
                };
                match ipc.request(req) {
                    Ok(_) => output.success(
                        "brightness.set",
                        &serde_json::json!({ "value": value }),
                        Some(&format!("Set brightness to {value}%")),
                        Vec::new(),
                    ),
                    Err((code, msg)) => output.error(
                        "brightness.set",
                        "daemon_unavailable",
                        &msg,
                        None,
                        Vec::new(),
                        code,
                    ),
                }
            }
        },
        Commands::Doctor { fix, first_login } => {
            let doctor = DoctorChecker::new();
            let (items, has_fail) = if first_login {
                doctor.run_first_login_report(fix)
            } else {
                let items = doctor.run_diagnostics(fix);
                let fail = items
                    .iter()
                    .any(|i| i.status == adapters::doctor::DiagnosticStatus::Fail);
                (items, fail)
            };
            let report_str = doctor.format_report(&items);
            if has_fail {
                output.error(
                    "doctor",
                    "diagnostics_failed",
                    &report_str,
                    Some(serde_json::to_value(&items).unwrap_or_default()),
                    Vec::new(),
                    EXIT_FAILURE,
                )
            } else {
                output.success("doctor", &items, Some(&report_str), Vec::new())
            }
        }
        Commands::Ext { command } => {
            let ext = ExtAdapter::new();
            let op = match command {
                ExtCommands::Check { path } => ext.check(path.as_deref()),
                ExtCommands::Pack { path, output } => ext.pack(path.as_deref(), output.as_deref()),
                ExtCommands::Dev { path } => ext.dev(&path),
                ExtCommands::Reload { id } => ext.reload(id.as_deref()),
                ExtCommands::Logs { id: _, follow } if cli.json && follow => {
                    adapters::ext::ExtOpResult {
                        success: false,
                        data: serde_json::Value::Null,
                        human_message: "--json cannot be combined with --follow".into(),
                        warnings: Vec::new(),
                        exit_code: EXIT_INVALID_ARGS,
                    }
                }
                ExtCommands::Logs { id, follow } => ext.logs(id.as_deref(), follow),
                ExtCommands::List { dev } => ext.list(dev),
                ExtCommands::Search { query } => {
                    let snapshot = shilpo_ext::ExtensionCatalog::open_default().snapshot();
                    let q = query.unwrap_or_default().to_lowercase();
                    let matches = snapshot
                        .discover
                        .into_iter()
                        .filter(|e| {
                            e.release.id.to_string().to_lowercase().contains(&q)
                                || e.release.name.to_lowercase().contains(&q)
                        })
                        .collect::<Vec<_>>();
                    adapters::ext::ExtOpResult {
                        success: true,
                        data: serde_json::to_value(&matches).unwrap_or_default(),
                        human_message: format!("Found {} matching extension(s)", matches.len()),
                        warnings: Vec::new(),
                        exit_code: 0,
                    }
                }
                ExtCommands::Info { id } => {
                    let snapshot = shilpo_ext::ExtensionCatalog::open_default().snapshot();
                    if let Some(item) = snapshot
                        .discover
                        .into_iter()
                        .find(|e| e.release.id.to_string() == id)
                    {
                        adapters::ext::ExtOpResult {
                            success: true,
                            data: serde_json::to_value(&item).unwrap_or_default(),
                            human_message: format!(
                                "Name: {}\nID: {}\nVersion: {}",
                                item.release.name, item.release.id, item.release.version
                            ),
                            warnings: Vec::new(),
                            exit_code: 0,
                        }
                    } else {
                        adapters::ext::ExtOpResult {
                            success: false,
                            data: serde_json::Value::Null,
                            human_message: format!("Extension '{id}' not found in catalog"),
                            warnings: Vec::new(),
                            exit_code: 1,
                        }
                    }
                }
                ExtCommands::Install { target, hash } => ext.install(&target, hash.as_deref()),
                ExtCommands::Update { id, all, dry_run } => ext.update(id.as_deref(), all, dry_run),
                ExtCommands::Enable { id } => ext.enable(&id),
                ExtCommands::Disable { id } => ext.disable(&id),
                ExtCommands::Approve { id, grant_all } => ext.approve(&id, grant_all),
                ExtCommands::Rollback { id } => ext.rollback(&id),
                ExtCommands::Uninstall { id } => ext.uninstall(&id),
                ExtCommands::CheckUpdates => ext.check_updates(),
                ExtCommands::Channel { id, channel } => ext.channel(&id, channel.as_deref()),
                ExtCommands::Source { args } => ext.source(&args),
                ExtCommands::RefreshSources => ext.refresh_sources(),
                ExtCommands::Sign {
                    package,
                    key,
                    publisher,
                } => ext.sign(&package, &key, &publisher),
                ExtCommands::Keygen { output } => ext.keygen(&output),
            };

            if op.success {
                output.success("ext", &op.data, Some(&op.human_message), op.warnings)
            } else {
                output.error(
                    "ext",
                    "extension_operation_failed",
                    &op.human_message,
                    Some(op.data),
                    op.warnings,
                    op.exit_code,
                )
            }
        }
        Commands::Capture { action } => {
            let ipc = IpcAdapter::new();
            let intent = match action {
                CaptureAction::Region => shilpo_capture::CaptureIntent::Clipboard,
                CaptureAction::Edit => shilpo_capture::CaptureIntent::Annotation,
                CaptureAction::Ocr => shilpo_capture::CaptureIntent::Ocr,
                CaptureAction::Menu => shilpo_capture::CaptureIntent::Menu,
            };
            match ipc.capture(intent) {
                Ok(resp) => {
                    let text = match &resp.result {
                        Some(shilpo_services::IpcResult::Accepted) => {
                            "Capture request accepted by shilpo-shell".into()
                        }
                        result => format!("Capture result: {result:?}"),
                    };
                    let result_value = serde_json::to_value(&resp.result).unwrap_or_default();
                    output.success("capture", &result_value, Some(&text), Vec::new())
                }
                Err((code, msg)) => {
                    output.error("capture", "ipc_failed", &msg, None, Vec::new(), code)
                }
            }
        }
        Commands::Record { action } => {
            let ipc = IpcAdapter::new();
            let cmd = match action {
                RecordAction::Toggle => {
                    match ipc.record(shilpo_capture::RecordingCommand::Status) {
                        Ok(response) => match response.result {
                            Some(shilpo_services::IpcResult::Record(state))
                                if state.is_stoppable() =>
                            {
                                Ok(shilpo_capture::RecordingCommand::Stop)
                            }
                            Some(shilpo_services::IpcResult::Record(_)) => {
                                Ok(shilpo_capture::RecordingCommand::Start(
                                    shilpo_capture::RecordingRequest {
                                        source: shilpo_capture::RecordingSource::primary(),
                                        audio: shilpo_capture::AudioSource::System,
                                    },
                                ))
                            }
                            result => Err((
                                1,
                                format!("unexpected recording status response: {result:?}"),
                            )),
                        },
                        Err(error) => Err(error),
                    }
                }
                RecordAction::Start => Ok(shilpo_capture::RecordingCommand::Start(
                    shilpo_capture::RecordingRequest {
                        source: shilpo_capture::RecordingSource::primary(),
                        audio: shilpo_capture::AudioSource::System,
                    },
                )),
                RecordAction::Pause => Ok(shilpo_capture::RecordingCommand::Pause),
                RecordAction::Resume => Ok(shilpo_capture::RecordingCommand::Resume),
                RecordAction::Stop => Ok(shilpo_capture::RecordingCommand::Stop),
                RecordAction::Cancel => Ok(shilpo_capture::RecordingCommand::Cancel),
                RecordAction::Status => Ok(shilpo_capture::RecordingCommand::Status),
            };
            let result = cmd.and_then(|cmd| ipc.record(cmd));
            match result {
                Ok(resp) => {
                    let text = format!("Recording result: {:?}", resp.result);
                    output.success(
                        "record",
                        &serde_json::to_value(&resp.result).unwrap_or_default(),
                        Some(&text),
                        Vec::new(),
                    )
                }
                Err((code, msg)) => {
                    output.error("record", "ipc_failed", &msg, None, Vec::new(), code)
                }
            }
        }
    };

    std::process::exit(exit_code);
}

fn parse_duration(s: Option<&str>) -> Result<Duration, String> {
    let Some(s) = s else {
        return Ok(Duration::from_secs(10));
    };
    let s = s.trim();
    if let Some(rest) = s.strip_suffix("ms") {
        rest.parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| format!("invalid timeout duration '{s}'"))
    } else if let Some(rest) = s.strip_suffix('s') {
        rest.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| format!("invalid timeout duration '{s}'"))
    } else {
        s.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| format!("invalid timeout duration '{s}'"))
    }
}
