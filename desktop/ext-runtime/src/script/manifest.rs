use std::{collections::HashSet, fmt, path::Path};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use shilpo_ext_api::{ContributionId, ExtensionId};

pub const SCRIPT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptManifestError {
    Parse(String),
    Validation(String),
}

impl fmt::Display for ScriptManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "script manifest parse error: {message}"),
            Self::Validation(message) => {
                write!(formatter, "script manifest validation error: {message}")
            }
        }
    }
}

impl std::error::Error for ScriptManifestError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptManifest {
    pub schema_version: u32,
    pub id: ExtensionId,
    pub name: String,
    #[schemars(with = "String")]
    pub version: Version,
    pub runtime: ScriptRuntimeConfig,
    pub contributions: ScriptContributions,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptRuntimeConfig {
    pub mode: ScriptMode,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<u64>,
    pub timeout_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScriptMode {
    Poll,
    Stream,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptContributions {
    pub bar_widgets: Vec<ScriptBarWidgetContribution>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptBarWidgetContribution {
    pub id: ContributionId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ScriptManifest {
    pub fn from_toml(toml_str: &str) -> Result<Self, ScriptManifestError> {
        toml::from_str::<Self>(toml_str)
            .map_err(|error| ScriptManifestError::Parse(error.to_string()))
    }

    pub fn validate(&self, bundle_root: &Path) -> Result<(), ScriptManifestError> {
        let invalid = |message| Err(ScriptManifestError::Validation(message));
        if self.schema_version != SCRIPT_SCHEMA_VERSION {
            return invalid(format!(
                "unsupported schema_version {}; expected 1",
                self.schema_version
            ));
        }

        let exec_str = &self.runtime.executable;
        if exec_str.is_empty() {
            return invalid("executable path cannot be empty".into());
        }
        if exec_str.contains('\0') {
            return invalid("executable path contains NUL byte".into());
        }
        let exec_path = Path::new(exec_str);
        if exec_path.is_absolute() {
            return invalid(format!("executable path '{exec_str}' must be relative"));
        }
        for comp in exec_path.components() {
            if matches!(comp, std::path::Component::ParentDir) {
                return invalid(format!("executable path '{exec_str}' cannot contain '..'"));
            }
        }

        let full_exec_path = bundle_root.join(exec_path);
        if !full_exec_path.exists() {
            return invalid(format!(
                "executable file '{}' does not exist in bundle root",
                full_exec_path.display()
            ));
        }
        if !full_exec_path.is_file() {
            return invalid(format!(
                "executable path '{}' is not a regular file",
                full_exec_path.display()
            ));
        }

        if let (Ok(canonical_root), Ok(canonical_exec)) =
            (bundle_root.canonicalize(), full_exec_path.canonicalize())
        {
            if !canonical_exec.starts_with(&canonical_root) {
                return invalid(format!(
                    "executable path '{}' escapes bundle root via symlink",
                    exec_str
                ));
            }
        } else {
            return invalid("failed to canonicalize bundle paths".into());
        }

        if self.contributions.bar_widgets.is_empty() {
            return invalid("bundle must declare at least one bar_widget contribution".into());
        }
        let mut seen_ids = HashSet::new();
        for widget in &self.contributions.bar_widgets {
            if !seen_ids.insert(widget.id.clone()) {
                return invalid(format!("duplicate contribution ID '{}'", widget.id));
            }
        }

        if !(100..=60_000).contains(&self.runtime.timeout_ms) {
            return invalid(format!(
                "timeout_ms must be between 100 and 60,000 (got {})",
                self.runtime.timeout_ms
            ));
        }

        match self.runtime.mode {
            ScriptMode::Poll => {
                let interval = self.runtime.interval_ms.ok_or_else(|| {
                    ScriptManifestError::Validation(
                        "interval_ms is required for poll mode".to_string(),
                    )
                })?;
                if !(1_000..=86_400_000).contains(&interval) {
                    return invalid(format!(
                        "interval_ms must be between 1,000 and 86,400,000 (got {interval})"
                    ));
                }
                if self.runtime.timeout_ms >= interval {
                    return invalid(format!(
                        "timeout_ms ({}) must be strictly less than interval_ms ({interval})",
                        self.runtime.timeout_ms
                    ));
                }
            }
            ScriptMode::Stream => {
                if self.runtime.interval_ms.is_some() {
                    return invalid("interval_ms is not permitted for stream mode".into());
                }
            }
        }

        Ok(())
    }
}
