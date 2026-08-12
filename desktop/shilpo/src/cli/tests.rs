use crate::adapters::ConfigMigrateAdapter;
use crate::args::{Cli, Commands, ConfigCommands, ModeValue, ShellCommands, VisibilityAction};
use crate::output::{CliOutput, EXIT_FAILURE, EXIT_INVALID_ARGS, JsonEnvelope};
use clap::Parser;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_cli_parser_shell_subcommands() {
    let cli = Cli::try_parse_from(["shilpo", "shell", "status"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Shell {
            command: ShellCommands::Status
        })
    ));

    let cli = Cli::try_parse_from(["shilpo", "shell", "start"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Shell {
            command: ShellCommands::Start
        })
    ));

    let cli = Cli::try_parse_from(["shilpo", "shell", "stop"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Shell {
            command: ShellCommands::Stop
        })
    ));

    let cli = Cli::try_parse_from(["shilpo", "shell", "restart"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Shell {
            command: ShellCommands::Restart
        })
    ));

    let cli = Cli::try_parse_from([
        "shilpo", "shell", "logs", "--follow", "--since", "10m", "-n", "50",
    ])
    .unwrap();
    if let Some(Commands::Shell {
        command:
            ShellCommands::Logs {
                follow,
                since,
                lines,
            },
    }) = cli.command
    {
        assert!(follow);
        assert_eq!(since, Some("10m".to_string()));
        assert_eq!(lines, Some(50));
    } else {
        panic!("Expected Shell Logs command");
    }

    let cli = Cli::try_parse_from(["shilpo", "shell", "telemetry"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Shell {
            command: ShellCommands::Telemetry
        })
    ));
}

#[test]
fn test_cli_parser_ui_visibility_actions() {
    let cli = Cli::try_parse_from(["shilpo", "overview", "show"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Overview {
            action: VisibilityAction::Show
        })
    ));

    let cli = Cli::try_parse_from(["shilpo", "bar", "toggle"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Bar {
            action: VisibilityAction::Toggle
        })
    ));
}

#[test]
fn test_cli_parser_workspace_and_window() {
    let cli = Cli::try_parse_from(["shilpo", "workspace", "focus", "42"]).unwrap();
    if let Some(Commands::Workspace {
        command: crate::args::WorkspaceCommands::Focus { id },
    }) = cli.command
    {
        assert_eq!(id, 42);
    } else {
        panic!("Expected Workspace Focus");
    }

    let cli = Cli::try_parse_from(["shilpo", "workspace", "create"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Workspace {
            command: crate::args::WorkspaceCommands::Create
        })
    ));

    let cli = Cli::try_parse_from(["shilpo", "window", "focus-previous"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Window {
            command: crate::args::WindowCommands::FocusPrevious
        })
    ));

    let cli = Cli::try_parse_from(["shilpo", "window", "move", "10", "--workspace", "3"]).unwrap();
    if let Some(Commands::Window {
        command: crate::args::WindowCommands::Move { id, workspace },
    }) = cli.command
    {
        assert_eq!(id, 10);
        assert_eq!(workspace, 3);
    } else {
        panic!("Expected Window Move");
    }
}

#[test]
fn test_cli_parser_theme_and_doctor() {
    let cli = Cli::try_parse_from(["shilpo", "theme", "mode", "set", "dark"]).unwrap();
    if let Some(Commands::Theme {
        command:
            crate::args::ThemeCommands::Mode {
                action: crate::args::ThemeModeAction::Set { mode },
            },
    }) = cli.command
    {
        assert_eq!(mode, ModeValue::Dark);
    } else {
        panic!("Expected Theme Mode Set Dark");
    }

    let cli = Cli::try_parse_from(["shilpo", "theme", "wallpaper", "get"]).unwrap();
    if let Some(Commands::Theme {
        command: crate::args::ThemeCommands::Wallpaper { action },
    }) = cli.command
    {
        assert!(matches!(action, crate::args::ThemeWallpaperAction::Get));
    } else {
        panic!("Expected Theme Wallpaper Get");
    }

    let cli = Cli::try_parse_from(["shilpo", "doctor", "--fix", "--first-login"]).unwrap();
    if let Some(Commands::Doctor {
        fix,
        first_login,
        telemetry: _,
    }) = cli.command
    {
        assert!(fix);
        assert!(first_login);
    } else {
        panic!("Expected Doctor with --fix and --first-login");
    }
}

#[test]
fn test_removed_legacy_commands_fail_parsing() {
    assert!(Cli::try_parse_from(["shilpo", "msg", "get-status"]).is_err());
    assert!(Cli::try_parse_from(["shilpo", "msg", "toggle-bar"]).is_err());
}

#[test]
fn test_json_and_quiet_conflict() {
    let res = CliOutput::new(true, true);
    assert!(res.is_err());
    let (code, msg) = res.unwrap_err();
    assert_eq!(code, EXIT_INVALID_ARGS);
    assert!(msg.contains("--json") && msg.contains("--quiet"));
}

#[test]
fn test_json_envelope_serialization() {
    let env = JsonEnvelope {
        schema_version: 1,
        ok: true,
        command: "shell.status".into(),
        data: serde_json::json!({ "running": true }),
        warnings: vec!["test warning".into()],
        error: None,
    };
    let val = serde_json::to_value(&env).unwrap();
    assert_eq!(val["schema_version"], 1);
    assert_eq!(val["ok"], true);
    assert_eq!(val["command"], "shell.status");
    assert_eq!(val["data"]["running"], true);
    assert_eq!(val["warnings"][0], "test warning");
    assert!(val.get("error").unwrap().is_null());
}

#[test]
fn test_json_error_envelope_is_machine_readable() {
    let output = CliOutput {
        json: true,
        quiet: false,
    };
    assert_eq!(
        output.error(
            "usage",
            "usage.invalid_timeout",
            "invalid timeout duration 'nope'",
            None,
            Vec::new(),
            EXIT_INVALID_ARGS,
        ),
        EXIT_INVALID_ARGS
    );
}

#[test]
fn test_parse_duration_units() {
    assert_eq!(
        crate::parse_duration(Some("10s")),
        Ok(std::time::Duration::from_secs(10))
    );
    assert_eq!(
        crate::parse_duration(Some("500ms")),
        Ok(std::time::Duration::from_millis(500))
    );
    assert_eq!(
        crate::parse_duration(Some("5")),
        Ok(std::time::Duration::from_secs(5))
    );
    assert_eq!(
        crate::parse_duration(None),
        Ok(std::time::Duration::from_secs(10))
    );
    assert!(crate::parse_duration(Some("nope")).is_err());
}

#[test]
fn test_cli_parser_config_migrate_and_dry_run() {
    let cli = Cli::try_parse_from(["shilpo", "config", "migrate"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Config {
            command: ConfigCommands::Migrate { dry_run: false }
        })
    ));

    let cli = Cli::try_parse_from(["shilpo", "config", "migrate", "--dry-run"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Config {
            command: ConfigCommands::Migrate { dry_run: true }
        })
    ));

    assert!(Cli::try_parse_from(["shilpo", "config", "migrate", "--bogus"]).is_err());
    assert!(Cli::try_parse_from(["shilpo", "config", "migrate", "extra"]).is_err());
    assert!(Cli::try_parse_from(["shilpo", "config", "migrate", "--json"]).is_ok());
}

fn cli_tmp_primary(content: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, content).unwrap();
    (dir, path)
}

#[test]
fn test_cli_config_migrate_current_contract() {
    let (_dir, path) = cli_tmp_primary("version = 1\n\n[bar]\nheight = 48\n");
    let result = ConfigMigrateAdapter::run(&path, false);
    assert!(result.success);
    assert_eq!(result.exit_code, 0);
    assert!(
        result
            .human_message
            .contains("already at the latest schema version 1")
    );
    assert!(result.warnings.is_empty());

    let data = result.data.as_object().unwrap();
    assert_eq!(data["mode"], "apply");
    assert_eq!(data["changed"], false);
    assert_eq!(data["path"], path.display().to_string());
    assert_eq!(data["from_version"], 1);
    assert_eq!(data["to_version"], 1);
    assert!(data["steps"].as_array().unwrap().is_empty());
    assert!(data["backup_path"].is_null());
    assert!(data["migrated_toml"].is_null());
}

#[test]
fn test_cli_config_migrate_preview_contract() {
    let (_dir, path) = cli_tmp_primary("[bar]\nheight = 48\n");
    let result = ConfigMigrateAdapter::run(&path, true);
    assert!(result.success);
    assert!(result.human_message.contains("schema 0 -> 1"));
    assert!(result.human_message.contains("v0 -> v1"));
    assert!(
        result
            .human_message
            .contains("--- migrated config.toml ---")
    );
    assert!(
        result
            .human_message
            .contains("--- end migrated config.toml ---")
    );
    assert!(result.human_message.contains("version = 1"));

    let data = result.data.as_object().unwrap();
    assert_eq!(data["mode"], "preview");
    assert_eq!(data["changed"], true);
    assert_eq!(data["from_version"], 0);
    assert_eq!(data["to_version"], 1);
    assert_eq!(data["steps"], serde_json::json!(["v0 -> v1"]));
    assert!(data["backup_path"].is_null());
    let migrated = data["migrated_toml"].as_str().unwrap();
    assert!(migrated.starts_with("version = 1"));

    // Dry-run is byte-for-byte read-only.
    assert_eq!(fs::read_to_string(&path).unwrap(), "[bar]\nheight = 48\n");
    let names: Vec<String> = fs::read_dir(_dir.path())
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    assert_eq!(names, vec!["config.toml"]);
}

#[test]
fn test_cli_config_migrate_applied_contract() {
    let (_dir, path) = cli_tmp_primary("[bar]\nheight = 48\n");
    let result = ConfigMigrateAdapter::run(&path, false);
    assert!(result.success);
    assert!(result.human_message.contains("Migrated"));
    assert!(result.human_message.contains("backup:"));

    let data = result.data.as_object().unwrap();
    assert_eq!(data["mode"], "apply");
    assert_eq!(data["changed"], true);
    assert!(data["backup_path"].as_str().unwrap().contains(".bak."));
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .starts_with("version = 1")
    );
}

#[test]
fn test_cli_config_migrate_failure_contract() {
    let (_dir, path) = cli_tmp_primary("version = 9999\n");
    let result = ConfigMigrateAdapter::run(&path, false);
    assert!(!result.success);
    assert_eq!(result.exit_code, EXIT_FAILURE);
    assert_eq!(result.error_code, "config.migration.future_version");
    assert!(result.human_message.contains("config migration failed"));
    assert_eq!(result.data["path"], path.display().to_string());
    // Failure never writes: the future-version file is untouched.
    assert_eq!(fs::read_to_string(&path).unwrap(), "version = 9999\n");
}
