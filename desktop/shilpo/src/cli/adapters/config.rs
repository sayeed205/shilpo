//! CLI adapter for `shilpo config validate` and `shilpo config effective [--origins]`.
//!
//! Accepts explicit primary paths so tests never touch real user configuration
//! or process-global XDG variables.

use std::path::Path;

use serde_json::Value;

use crate::{
    cli::output::EXIT_FAILURE,
    config::{ConfigInspectionError, ConfigResolver, EffectiveWithOriginsReport},
};

/// Structured result of a config CLI inspection command (`validate` or `effective`).
#[derive(Debug)]
pub struct ConfigInspectResult {
    pub success: bool,
    pub command: &'static str,
    pub data: Value,
    pub human_message: String,
    pub warnings: Vec<String>,
    pub exit_code: i32,
    pub error_code: &'static str,
    pub error_details: Option<Value>,
}

/// Adapter over the strict config inspection seam in [`ConfigResolver`].
#[derive(Debug, Default)]
pub struct ConfigAdapter;

impl ConfigAdapter {
    pub fn validate(primary_path: &Path) -> ConfigInspectResult {
        let resolver = ConfigResolver::from_primary_path(primary_path);
        match resolver.inspect_strict() {
            Ok(inspection) => {
                let warnings = inspection
                    .unknown_keys
                    .iter()
                    .map(|w| w.describe())
                    .collect();
                let data = serde_json::json!({
                    "valid": true,
                    "path": primary_path.display().to_string(),
                    "sources": inspection.sources_loaded,
                });
                ConfigInspectResult {
                    success: true,
                    command: "config.validate",
                    data,
                    human_message: format!("Configuration at {} is valid", primary_path.display()),
                    warnings,
                    exit_code: 0,
                    error_code: "",
                    error_details: None,
                }
            }
            Err(error) => format_error("config.validate", &error),
        }
    }

    pub fn effective(primary_path: &Path, origins: bool) -> ConfigInspectResult {
        let resolver = ConfigResolver::from_primary_path(primary_path);
        match resolver.inspect_strict() {
            Ok(inspection) => {
                let warnings = inspection
                    .unknown_keys
                    .iter()
                    .map(|w| w.describe())
                    .collect();
                let report = EffectiveWithOriginsReport::from_snapshot(&inspection.snapshot);

                let effective_toml = match canonical_effective_toml(&inspection.snapshot.config) {
                    Ok(toml) => toml,
                    Err(error) => return format_error("config.effective", &error),
                };

                let human_message = if origins {
                    let mut provenance_comments = String::new();
                    provenance_comments.push_str("\n# --- Provenance ---\n");
                    for (path, loc) in &report.origins {
                        let line_col_str = match (loc.line, loc.column) {
                            (Some(line), Some(col)) => format!(":{line}:{col}"),
                            _ => String::new(),
                        };
                        provenance_comments
                            .push_str(&format!("# {path} => {}{line_col_str}\n", loc.source));
                    }
                    format!("{}{provenance_comments}", effective_toml.trim_end())
                } else {
                    effective_toml
                };

                let data = if origins {
                    serde_json::json!({
                        "path": primary_path.display().to_string(),
                        "sources": inspection.sources_loaded,
                        "effective": inspection.snapshot.config,
                        "origins": report.origins,
                    })
                } else {
                    serde_json::json!({
                        "path": primary_path.display().to_string(),
                        "sources": inspection.sources_loaded,
                        "effective": inspection.snapshot.config,
                    })
                };

                ConfigInspectResult {
                    success: true,
                    command: "config.effective",
                    data,
                    human_message,
                    warnings,
                    exit_code: 0,
                    error_code: "",
                    error_details: None,
                }
            }
            Err(error) => format_error("config.effective", &error),
        }
    }
}

fn canonical_effective_toml(
    config: &crate::config::ShellConfig,
) -> Result<String, ConfigInspectionError> {
    let serialized =
        toml::to_string_pretty(config).map_err(|error| ConfigInspectionError::Serialization {
            message: error.to_string(),
        })?;
    let value: toml::Value =
        toml::from_str(&serialized).map_err(|error| ConfigInspectionError::Serialization {
            message: error.to_string(),
        })?;
    toml::to_string_pretty(&value).map_err(|error| ConfigInspectionError::Serialization {
        message: error.to_string(),
    })
}

fn format_error(command: &'static str, error: &ConfigInspectionError) -> ConfigInspectResult {
    ConfigInspectResult {
        success: false,
        command,
        data: Value::Null,
        human_message: format!("{error}"),
        warnings: Vec::new(),
        exit_code: EXIT_FAILURE,
        error_code: error.code(),
        error_details: Some(error.details()),
    }
}
