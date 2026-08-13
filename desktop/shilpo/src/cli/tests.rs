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

#[test]
fn test_cli_parser_config_validate_and_effective() {
    let cli = Cli::try_parse_from(["shilpo", "config", "validate"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Config {
            command: ConfigCommands::Validate
        })
    ));

    let cli = Cli::try_parse_from(["shilpo", "config", "effective"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Config {
            command: ConfigCommands::Effective { origins: false }
        })
    ));

    let cli = Cli::try_parse_from(["shilpo", "config", "effective", "--origins"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Config {
            command: ConfigCommands::Effective { origins: true }
        })
    ));

    assert!(Cli::try_parse_from(["shilpo", "config", "effective", "--bogus"]).is_err());
    assert!(Cli::try_parse_from(["shilpo", "config", "validate", "extra"]).is_err());
}

#[test]
fn test_cli_config_missing_and_empty_primary_resolves_defaults_read_only() {
    use crate::adapters::ConfigAdapter;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");

    // 1. Non-existent primary
    let res_val = ConfigAdapter::validate(&path);
    assert!(res_val.success);
    assert_eq!(res_val.exit_code, 0);
    assert_eq!(res_val.data["valid"], true);
    assert!(!path.exists());

    let res_eff = ConfigAdapter::effective(&path, false);
    assert!(res_eff.success);
    assert_eq!(res_eff.exit_code, 0);
    assert!(!path.exists());

    // 2. Empty string primary
    fs::write(&path, "").unwrap();
    let res_val_empty = ConfigAdapter::validate(&path);
    assert!(res_val_empty.success);
    assert_eq!(fs::read_to_string(&path).unwrap(), "");
}

#[test]
fn test_cli_config_layered_precedence_order() {
    use crate::adapters::ConfigAdapter;

    let dir = TempDir::new().unwrap();
    let primary = dir.path().join("config.toml");
    fs::write(&primary, "version = 1\n[bar]\nheight = 30\n").unwrap();

    let conf_d = dir.path().join("conf.d");
    fs::create_dir(&conf_d).unwrap();
    fs::write(conf_d.join("01-bar.toml"), "[bar]\nheight = 40\n").unwrap();
    fs::write(conf_d.join("02-bar.toml"), "[bar]\nheight = 50\n").unwrap();

    let overrides = dir.path().join("overrides.toml");
    fs::write(&overrides, "[bar]\nheight = 60\n").unwrap();

    let res = ConfigAdapter::effective(&primary, true);
    assert!(res.success);
    assert_eq!(res.data["effective"]["bar"]["height"], 60);

    let origins = res.data["origins"].as_object().unwrap();
    assert!(origins["bar.height"]["source"]["Overrides"].is_object());
}

#[test]
fn test_cli_config_unknown_keys_warning_contract() {
    use crate::adapters::ConfigAdapter;

    let dir = TempDir::new().unwrap();
    let primary = dir.path().join("config.toml");
    fs::write(&primary, "version = 1\nbaer = 1\n").unwrap();

    let conf_d = dir.path().join("conf.d");
    fs::create_dir(&conf_d).unwrap();
    fs::write(conf_d.join("01-bar.toml"), "[bar]\nheigth = 2\n").unwrap();

    let overrides = dir.path().join("overrides.toml");
    fs::write(&overrides, "[bar]\nxyz = 3\n").unwrap();

    let res_val = ConfigAdapter::validate(&primary);
    assert!(res_val.success);
    assert_eq!(res_val.exit_code, 0);
    assert_eq!(res_val.warnings.len(), 3);
    assert!(res_val.warnings[0].contains("baer"));
    assert!(res_val.warnings[1].contains("bar.heigth"));

    let res_eff = ConfigAdapter::effective(&primary, true);
    assert!(res_eff.success);
    let effective_json = serde_json::to_string(&res_eff.data["effective"]).unwrap();
    assert!(!effective_json.contains("baer"));
    assert!(!effective_json.contains("heigth"));
    assert!(!effective_json.contains("xyz"));
}

#[test]
fn test_cli_config_primary_syntax_failure_blocking() {
    use crate::adapters::ConfigAdapter;

    let (_dir, primary) = cli_tmp_primary("version = 1\ninvalid = [toml syntax\n");
    let result = ConfigAdapter::validate(&primary);
    assert!(!result.success);
    assert_eq!(result.exit_code, EXIT_FAILURE);
    assert_eq!(result.error_code, "config.validation.parse_failed");
    assert_eq!(
        result.error_details.unwrap()["path"],
        primary.display().to_string()
    );
    assert_eq!(
        fs::read_to_string(&primary).unwrap(),
        "version = 1\ninvalid = [toml syntax\n"
    );
}

#[test]
fn test_cli_config_fragment_and_override_syntax_type_failures_blocking() {
    use crate::adapters::ConfigAdapter;

    let dir = TempDir::new().unwrap();
    let primary = dir.path().join("config.toml");
    fs::write(&primary, "version = 1\n").unwrap();

    let conf_d = dir.path().join("conf.d");
    fs::create_dir(&conf_d).unwrap();
    let bad_frag = conf_d.join("01-bad.toml");
    fs::write(&bad_frag, "invalid = [syntax\n").unwrap();

    let res = ConfigAdapter::validate(&primary);
    assert!(!res.success);
    assert_eq!(res.exit_code, EXIT_FAILURE);
    assert_eq!(res.error_code, "config.validation.parse_failed");
    assert_eq!(
        res.error_details.unwrap()["path"],
        bad_frag.display().to_string()
    );

    // Clean bad fragment and add type error override
    fs::remove_file(&bad_frag).unwrap();
    let overrides = dir.path().join("overrides.toml");
    fs::write(&overrides, "bar = \"not a table\"\n").unwrap();

    let res_type = ConfigAdapter::validate(&primary);
    assert!(!res_type.success);
    assert_eq!(res_type.exit_code, EXIT_FAILURE);
    assert!(res_type.error_code.starts_with("config.validation."));
}

#[test]
fn test_cli_config_semantic_validation_no_scoped_recovery() {
    use crate::adapters::ConfigAdapter;

    let (_dir, primary) = cli_tmp_primary("version = 1\n[bar]\nheight = 1\n");
    let result = ConfigAdapter::validate(&primary);
    assert!(!result.success);
    assert_eq!(result.exit_code, EXIT_FAILURE);
    assert_eq!(result.error_code, "config.validation.semantic_failed");
    assert!(result.human_message.contains("bar.height"));
}

#[test]
fn test_cli_config_version_migration_and_future_semantics() {
    use crate::adapters::ConfigAdapter;

    // Missing version key in non-empty primary
    let (_dir, legacy) = cli_tmp_primary("[bar]\nheight = 40\n");
    let res_legacy = ConfigAdapter::validate(&legacy);
    assert!(!res_legacy.success);
    assert_eq!(res_legacy.exit_code, EXIT_FAILURE);
    assert_eq!(
        res_legacy.error_code,
        "config.validation.migration_required"
    );
    assert!(res_legacy.human_message.contains("shilpo config migrate"));

    // Invalid negative version
    let (_dir, invalid) = cli_tmp_primary("version = -1\n");
    let res_inv = ConfigAdapter::validate(&invalid);
    assert!(!res_inv.success);
    assert_eq!(res_inv.exit_code, EXIT_FAILURE);
    assert_eq!(res_inv.error_code, "config.validation.invalid_version");

    // Future version
    let (_dir, future) = cli_tmp_primary("version = 999\n");
    let res_fut = ConfigAdapter::validate(&future);
    assert!(!res_fut.success);
    assert_eq!(res_fut.exit_code, EXIT_FAILURE);
    assert_eq!(res_fut.error_code, "config.validation.future_version");
}

#[test]
fn test_cli_config_version_in_fragment_or_override_rejected() {
    use crate::adapters::ConfigAdapter;

    let dir = TempDir::new().unwrap();
    let primary = dir.path().join("config.toml");
    fs::write(&primary, "version = 1\n").unwrap();

    let conf_d = dir.path().join("conf.d");
    fs::create_dir(&conf_d).unwrap();
    let frag = conf_d.join("01-bar.toml");
    fs::write(&frag, "version = 1\n[bar]\nheight = 40\n").unwrap();

    let res = ConfigAdapter::validate(&primary);
    assert!(!res.success);
    assert_eq!(res.exit_code, EXIT_FAILURE);
    assert_eq!(res.error_code, "config.validation.invalid_source_version");
    assert_eq!(
        res.error_details.unwrap()["path"],
        frag.display().to_string()
    );
}

#[test]
fn test_cli_config_effective_human_roundtrip() {
    use crate::adapters::ConfigAdapter;
    use crate::config::ShellConfig;

    let (_dir, primary) = cli_tmp_primary("version = 1\n[theme]\nfont_family = \"Inter\"\n");

    let res_no_orig = ConfigAdapter::effective(&primary, false);
    assert!(res_no_orig.success);
    let cfg_no_orig: ShellConfig = toml::from_str(&res_no_orig.human_message).unwrap();
    assert_eq!(cfg_no_orig.theme.font_family, "Inter");

    let res_orig = ConfigAdapter::effective(&primary, true);
    assert!(res_orig.success);
    let cfg_orig: ShellConfig = toml::from_str(&res_orig.human_message).unwrap();
    assert_eq!(cfg_orig.theme.font_family, "Inter");
    assert!(res_orig.human_message.contains("# --- Provenance ---"));
}

#[test]
fn test_cli_config_effective_origins_provenance_map() {
    use crate::adapters::ConfigAdapter;

    let (_dir, primary) = cli_tmp_primary("version = 1\n[theme]\nfont_family = \"Inter\"\n");
    let res = ConfigAdapter::effective(&primary, true);
    assert!(res.success);

    let origins = res.data["origins"].as_object().unwrap();
    assert!(origins.contains_key("theme.font_family"));
    assert!(origins["theme.font_family"]["source"]["Primary"].is_object());
    assert_eq!(origins["bar.height"]["source"], "Defaults");
}

#[test]
fn test_cli_config_json_envelopes_contract() {
    use crate::adapters::ConfigAdapter;

    let (_dir, primary) = cli_tmp_primary("version = 1\n");
    let res = ConfigAdapter::validate(&primary);
    assert!(res.success);
    assert_eq!(res.command, "config.validate");
    assert_eq!(res.data["valid"], true);
    assert!(!res.data["sources"].as_array().unwrap().is_empty());

    let (_dir, bad_primary) = cli_tmp_primary("version = 1\ninvalid = [syntax\n");
    let res_err = ConfigAdapter::validate(&bad_primary);
    assert!(!res_err.success);
    assert_eq!(res_err.command, "config.validate");
    assert_eq!(res_err.error_code, "config.validation.parse_failed");
    assert!(res_err.data.is_null());
}

#[test]
fn test_cli_config_quiet_mode_suppression() {
    use crate::output::CliOutput;

    let output = CliOutput::new(false, true).unwrap();
    let data = serde_json::json!({ "valid": true });
    let code = output.success(
        "config.validate",
        &data,
        Some("human valid message"),
        Vec::new(),
    );
    assert_eq!(code, 0);
}

#[test]
fn test_cli_config_read_only_invariants() {
    use crate::adapters::ConfigAdapter;

    let dir = TempDir::new().unwrap();
    let primary = dir.path().join("config.toml");
    let original_bytes = "version = 1\n[bar]\nheight = 40\n";
    fs::write(&primary, original_bytes).unwrap();

    let conf_d = dir.path().join("conf.d");
    fs::create_dir(&conf_d).unwrap();
    let frag_path = conf_d.join("01-bar.toml");
    let frag_bytes = "[bar]\nheight = 45\n";
    fs::write(&frag_path, frag_bytes).unwrap();

    let res = ConfigAdapter::effective(&primary, true);
    assert!(res.success);

    // Verify files remain byte-for-byte unchanged
    assert_eq!(fs::read_to_string(&primary).unwrap(), original_bytes);
    assert_eq!(fs::read_to_string(&frag_path).unwrap(), frag_bytes);
}

#[test]
fn test_cli_config_adapter_uses_shared_resolver_seam() {
    use crate::adapters::ConfigAdapter;
    use crate::config::ConfigResolver;

    let (_dir, primary) = cli_tmp_primary("version = 1\n[bar]\nheight = 50\n");

    let resolver = ConfigResolver::from_primary_path(&primary);
    let inspection = resolver.inspect_strict().unwrap();

    let res = ConfigAdapter::effective(&primary, false);
    assert_eq!(
        res.data["effective"]["bar"]["height"],
        inspection.snapshot.config.bar.height
    );
}

#[test]
fn test_cli_config_whitespace_primary_is_default_and_read_only() {
    use crate::adapters::ConfigAdapter;

    let dir = TempDir::new().unwrap();
    let primary = dir.path().join("config.toml");
    let original = "  \n\n\t";
    fs::write(&primary, original).unwrap();

    let result = ConfigAdapter::validate(&primary);
    assert!(result.success);
    assert_eq!(result.data["valid"], true);
    assert_eq!(fs::read_to_string(&primary).unwrap(), original);
}

#[test]
fn test_cli_config_errors_identify_fragment_or_override_source() {
    use crate::adapters::ConfigAdapter;

    let dir = TempDir::new().unwrap();
    let primary = dir.path().join("config.toml");
    fs::write(&primary, "version = 1\n").unwrap();

    let conf_d = dir.path().join("conf.d");
    fs::create_dir(&conf_d).unwrap();
    let fragment = conf_d.join("01-bad.toml");
    fs::write(&fragment, "bar = \"not a table\"\n").unwrap();

    let result = ConfigAdapter::validate(&primary);
    assert!(!result.success);
    assert_eq!(
        result.error_details.unwrap()["path"],
        fragment.display().to_string()
    );
}

#[test]
fn test_cli_config_effective_toml_is_deterministic_for_dynamic_maps() {
    use crate::adapters::ConfigAdapter;

    let dir = TempDir::new().unwrap();
    let primary = dir.path().join("config.toml");
    fs::write(
        &primary,
        "version = 1\n[outputs.zeta]\nenabled = true\n[outputs.alpha]\nenabled = true\n",
    )
    .unwrap();

    let first = ConfigAdapter::effective(&primary, false);
    let second = ConfigAdapter::effective(&primary, false);
    assert!(first.success && second.success);
    assert_eq!(first.human_message, second.human_message);
    assert!(
        first.human_message.find("[outputs.alpha]").unwrap()
            < first.human_message.find("[outputs.zeta]").unwrap()
    );
}
