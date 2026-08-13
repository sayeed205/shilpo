use crate::cli::output::EXIT_FAILURE;
use crate::config::{MigrationMode, MigrationOutcome, MigrationService};
use serde_json::Value;
use std::path::Path;

/// Result of a `shilpo config migrate` run, rendered for both the human and
/// JSON CLI contracts.
#[derive(Debug)]
pub struct ConfigMigrateResult {
    pub success: bool,
    pub data: Value,
    pub human_message: String,
    pub warnings: Vec<String>,
    pub exit_code: i32,
    /// Stable machine-readable error code (`config.migration.*`), only when
    /// `success` is false.
    pub error_code: &'static str,
}

/// CLI adapter over the migration service shared with shell startup. Accepts
/// an explicit primary path so tests never touch real user configuration or
/// process-global XDG variables.
#[derive(Debug, Default)]
pub struct ConfigMigrateAdapter;

impl ConfigMigrateAdapter {
    pub fn run(primary_path: &Path, dry_run: bool) -> ConfigMigrateResult {
        let mode = if dry_run {
            MigrationMode::Preview
        } else {
            MigrationMode::Apply
        };
        let service = MigrationService::for_primary_path(primary_path);
        match service.run(mode) {
            Ok(outcome) => {
                let warnings = outcome
                    .warnings
                    .iter()
                    .map(|warning| warning.describe())
                    .collect();
                ConfigMigrateResult {
                    success: true,
                    data: serde_json::to_value(&outcome).unwrap_or(Value::Null),
                    human_message: format_migrate_human(&outcome, dry_run),
                    warnings,
                    exit_code: 0,
                    error_code: "",
                }
            }
            Err(error) => ConfigMigrateResult {
                success: false,
                data: error
                    .path()
                    .map(|path| serde_json::json!({ "path": path.display().to_string() }))
                    .unwrap_or(Value::Null),
                human_message: format!("config migration failed: {error}"),
                warnings: Vec::new(),
                exit_code: EXIT_FAILURE,
                error_code: error.code(),
            },
        }
    }
}

/// Human-readable migration result. A dry-run preview that needs migration
/// prints the ordered step names plus the clearly delimited complete migrated
/// TOML, never just "would migrate".
pub fn format_migrate_human(outcome: &MigrationOutcome, dry_run: bool) -> String {
    let path = outcome.path.display();
    if !outcome.changed {
        return format!(
            "{path} is already at the latest schema version {}",
            outcome.to_version
        );
    }
    let steps = outcome
        .steps
        .iter()
        .map(|step| format!("    - {step}"))
        .collect::<Vec<_>>()
        .join("\n");
    if dry_run {
        let toml = outcome.migrated_toml.as_deref().unwrap_or_default();
        format!(
            "Migration preview for {path}\n  schema {} -> {}\n  steps:\n{steps}\n  --- migrated config.toml ---\n{toml}\n  --- end migrated config.toml ---",
            outcome.from_version, outcome.to_version
        )
    } else {
        let backup = outcome
            .backup_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".into());
        format!(
            "Migrated {path}\n  schema {} -> {}\n  steps:\n{steps}\n  backup: {backup}",
            outcome.from_version, outcome.to_version
        )
    }
}
