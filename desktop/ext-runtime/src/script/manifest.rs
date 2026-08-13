use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shilpo_ext_api::{ContributionId, ExtensionId};
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptManifest {
    pub schema_version: u32,
    pub id: ExtensionId,
    pub name: String,
    pub version: String,
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
    pub fn from_toml(toml_str: &str) -> Result<Self, String> {
        toml::from_str::<Self>(toml_str).map_err(|e| format!("manifest TOML error: {e}"))
    }

    pub fn validate(&self, bundle_root: &Path) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported schema_version {}; expected 1",
                self.schema_version
            ));
        }

        if semver::Version::parse(&self.version).is_err() {
            return Err(format!("invalid semver version '{}'", self.version));
        }

        let exec_str = &self.runtime.executable;
        if exec_str.is_empty() {
            return Err("executable path cannot be empty".into());
        }
        if exec_str.contains('\0') {
            return Err("executable path contains NUL byte".into());
        }
        let exec_path = Path::new(exec_str);
        if exec_path.is_absolute() {
            return Err(format!("executable path '{exec_str}' must be relative"));
        }
        for comp in exec_path.components() {
            if matches!(comp, std::path::Component::ParentDir) {
                return Err(format!("executable path '{exec_str}' cannot contain '..'"));
            }
        }

        let full_exec_path = bundle_root.join(exec_path);
        if !full_exec_path.exists() {
            return Err(format!(
                "executable file '{}' does not exist in bundle root",
                full_exec_path.display()
            ));
        }
        if !full_exec_path.is_file() {
            return Err(format!(
                "executable path '{}' is not a regular file",
                full_exec_path.display()
            ));
        }

        if let (Ok(canonical_root), Ok(canonical_exec)) =
            (bundle_root.canonicalize(), full_exec_path.canonicalize())
        {
            if !canonical_exec.starts_with(&canonical_root) {
                return Err(format!(
                    "executable path '{}' escapes bundle root via symlink",
                    exec_str
                ));
            }
        } else {
            return Err("failed to canonicalize bundle paths".into());
        }

        if self.contributions.bar_widgets.is_empty() {
            return Err("bundle must declare at least one bar_widget contribution".into());
        }
        let mut seen_ids = HashSet::new();
        for widget in &self.contributions.bar_widgets {
            if !seen_ids.insert(widget.id.clone()) {
                return Err(format!("duplicate contribution ID '{}'", widget.id));
            }
        }

        if !(100..=60_000).contains(&self.runtime.timeout_ms) {
            return Err(format!(
                "timeout_ms must be between 100 and 60,000 (got {})",
                self.runtime.timeout_ms
            ));
        }

        match self.runtime.mode {
            ScriptMode::Poll => {
                let interval = self
                    .runtime
                    .interval_ms
                    .ok_or_else(|| "interval_ms is required for poll mode".to_string())?;
                if !(1_000..=86_400_000).contains(&interval) {
                    return Err(format!(
                        "interval_ms must be between 1,000 and 86,400,000 (got {interval})"
                    ));
                }
                if self.runtime.timeout_ms >= interval {
                    return Err(format!(
                        "timeout_ms ({}) must be strictly less than interval_ms ({interval})",
                        self.runtime.timeout_ms
                    ));
                }
            }
            ScriptMode::Stream => {
                if self.runtime.interval_ms.is_some() {
                    return Err("interval_ms is not permitted for stream mode".into());
                }
            }
        }

        Ok(())
    }
}
