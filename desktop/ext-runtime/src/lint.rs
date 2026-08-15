use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use shilpo_ext_api::{Capability, ExtensionManifest};

use crate::CURRENT_SHILPO_VERSION;
use crate::build::{ExtensionLanguage, ExtensionProjectConfig};
use crate::wasm::WasmRuntime;

pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024; // 16 MiB
pub const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB
pub const WASM_LARGE_THRESHOLD_BYTES: u64 = 20 * 1024 * 1024; // 20 MiB

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for LintSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintDiagnostic {
    pub rule_id: String,
    pub severity: LintSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}

impl LintDiagnostic {
    pub fn error(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity: LintSeverity::Error,
            message: message.into(),
            path: None,
            line: None,
            column: None,
            remediation: None,
        }
    }

    pub fn warning(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity: LintSeverity::Warning,
            message: message.into(),
            path: None,
            line: None,
            column: None,
            remediation: None,
        }
    }

    pub fn info(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity: LintSeverity::Info,
            message: message.into(),
            path: None,
            line: None,
            column: None,
            remediation: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_line_col(mut self, line: u32, column: u32) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }
}

impl Ord for LintDiagnostic {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path
            .cmp(&other.path)
            .then_with(|| self.line.cmp(&other.line))
            .then_with(|| self.column.cmp(&other.column))
            .then_with(|| self.rule_id.cmp(&other.rule_id))
            .then_with(|| self.message.cmp(&other.message))
    }
}

impl PartialOrd for LintDiagnostic {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintReport {
    pub schema_version: u32,
    pub project_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_id: Option<String>,
    pub passed: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub diagnostics: Vec<LintDiagnostic>,
}

impl LintReport {
    pub fn from_diagnostics(
        project_path: impl Into<String>,
        extension_id: Option<String>,
        mut diagnostics: Vec<LintDiagnostic>,
        deny_warnings: bool,
    ) -> Self {
        diagnostics.sort();
        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == LintSeverity::Error)
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == LintSeverity::Warning)
            .count();
        let info_count = diagnostics
            .iter()
            .filter(|d| d.severity == LintSeverity::Info)
            .count();

        let passed = if deny_warnings {
            error_count == 0 && warning_count == 0
        } else {
            error_count == 0
        };

        Self {
            schema_version: 1,
            project_path: project_path.into(),
            extension_id,
            passed,
            error_count,
            warning_count,
            info_count,
            diagnostics,
        }
    }

    pub fn from_single_diagnostic(project_path: &Path, diagnostic: LintDiagnostic) -> Self {
        let path_str = if project_path.as_os_str().is_empty() {
            ".".to_string()
        } else {
            project_path.to_string_lossy().to_string()
        };
        Self::from_diagnostics(path_str, None, vec![diagnostic], false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LintOptions {
    pub deny_warnings: bool,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectionPolicy {
    Lint { deny_warnings: bool },
    Check,
}

pub struct CheckedExtensionData {
    pub manifest: ExtensionManifest,
    pub files: Vec<PathBuf>,
    pub report: LintReport,
}

pub fn inspect_extension(dir: &Path, policy: InspectionPolicy) -> LintReport {
    inspect_extension_full(dir, policy).2
}

pub fn inspect_extension_with_timeout(
    dir: &Path,
    deny_warnings: bool,
    timeout: Duration,
) -> LintReport {
    inspect_extension_full_with_timeout(dir, InspectionPolicy::Lint { deny_warnings }, timeout).2
}

pub fn inspect_extension_checked(
    dir: &Path,
) -> Result<CheckedExtensionData, (Option<String>, Vec<String>)> {
    let (manifest, files, report) = inspect_extension_full(dir, InspectionPolicy::Check);
    if report.passed
        && let Some(manifest) = manifest
    {
        Ok(CheckedExtensionData {
            manifest,
            files,
            report,
        })
    } else {
        let legacy_diagnostics = report
            .diagnostics
            .into_iter()
            .map(|d| format!("{}[{}]: {}", d.severity, d.rule_id, d.message))
            .collect();
        Err((report.extension_id, legacy_diagnostics))
    }
}

pub fn inspect_extension_full(
    dir: &Path,
    policy: InspectionPolicy,
) -> (Option<ExtensionManifest>, Vec<PathBuf>, LintReport) {
    inspect_extension_full_with_timeout(dir, policy, WasmRuntime::DEFAULT_VALIDATION_TIMEOUT)
}

fn inspect_extension_full_with_timeout(
    dir: &Path,
    policy: InspectionPolicy,
    validation_timeout: Duration,
) -> (Option<ExtensionManifest>, Vec<PathBuf>, LintReport) {
    let project_display = if dir.as_os_str().is_empty() {
        ".".to_string()
    } else {
        dir.to_string_lossy().to_string()
    };

    let deny_warnings = match policy {
        InspectionPolicy::Lint { deny_warnings } => deny_warnings,
        InspectionPolicy::Check => false,
    };

    let root = match dir.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            let diag = LintDiagnostic::error(
                "path.not-found",
                format!(
                    "failed to resolve project directory '{}': {error}",
                    dir.display()
                ),
            )
            .with_remediation("Ensure the specified project directory exists and is accessible");
            return (
                None,
                Vec::new(),
                LintReport::from_diagnostics(project_display, None, vec![diag], deny_warnings),
            );
        }
    };

    let project_display = root.to_string_lossy().to_string();

    if !root.is_dir() {
        let diag = LintDiagnostic::error(
            "path.not-directory",
            format!("specified path '{}' is not a directory", dir.display()),
        );
        return (
            None,
            Vec::new(),
            LintReport::from_diagnostics(project_display, None, vec![diag], deny_warnings),
        );
    }

    let mut diagnostics = Vec::new();

    // 1. Manifest Parsing and Syntax
    let manifest_path = root.join("extension.toml");
    let manifest_meta = match fs::symlink_metadata(&manifest_path) {
        Ok(meta) => meta,
        Err(error) => {
            let diag = LintDiagnostic::error(
                "manifest.missing",
                format!("manifest extension.toml is missing: {error}"),
            )
            .with_path("extension.toml")
            .with_remediation("Create an extension.toml manifest in the project root");
            return (
                None,
                Vec::new(),
                LintReport::from_diagnostics(project_display, None, vec![diag], deny_warnings),
            );
        }
    };

    if manifest_meta.file_type().is_symlink() || !manifest_meta.is_file() {
        let diag = LintDiagnostic::error(
            "manifest.not-file",
            "manifest extension.toml is not a regular file",
        )
        .with_path("extension.toml");
        return (
            None,
            Vec::new(),
            LintReport::from_diagnostics(project_display, None, vec![diag], deny_warnings),
        );
    }

    let manifest_source = match fs::read_to_string(&manifest_path) {
        Ok(source) => source,
        Err(error) => {
            let diag = LintDiagnostic::error(
                "manifest.read",
                format!("failed to read extension.toml: {error}"),
            )
            .with_path("extension.toml");
            return (
                None,
                Vec::new(),
                LintReport::from_diagnostics(project_display, None, vec![diag], deny_warnings),
            );
        }
    };

    if let Err(error) = toml::from_str::<toml::Value>(&manifest_source) {
        let mut diag =
            LintDiagnostic::error("manifest.syntax", error.to_string()).with_path("extension.toml");
        if let Some(span) = error.span() {
            let (line, col) = line_col_from_offset(&manifest_source, span.start);
            diag = diag.with_line_col(line, col);
        }
        return (
            None,
            Vec::new(),
            LintReport::from_diagnostics(project_display, None, vec![diag], deny_warnings),
        );
    }

    let manifest: ExtensionManifest = match toml::from_str(&manifest_source) {
        Ok(manifest) => manifest,
        Err(error) => {
            let diag = LintDiagnostic::error("manifest.invalid", error.to_string())
                .with_path("extension.toml");
            return (
                None,
                Vec::new(),
                LintReport::from_diagnostics(project_display, None, vec![diag], deny_warnings),
            );
        }
    };
    let canonical_validation_error = ExtensionManifest::from_toml(&manifest_source).err();

    let extension_id = Some(manifest.id.to_string());
    diagnostics.push(
        LintDiagnostic::info(
            "manifest.valid",
            format!("'{}' {}", manifest.id, manifest.version),
        )
        .with_path("extension.toml"),
    );
    if let Some(error) = canonical_validation_error {
        diagnostics.push(
            LintDiagnostic::error("manifest.invalid", error.to_string())
                .with_path("extension.toml"),
        );
    }

    // 2. Manifest Versioning & Compatibility
    if manifest.schema_version != 1 {
        diagnostics.push(
            LintDiagnostic::error(
                "manifest.unsupported-schema",
                format!(
                    "manifest schema_version {} is unsupported; expected 1",
                    manifest.schema_version
                ),
            )
            .with_path("extension.toml"),
        );
    }

    if manifest.api_version.to_string() != "0.1.0" {
        diagnostics.push(
            LintDiagnostic::error(
                "manifest.unsupported-api-version",
                format!(
                    "manifest api_version '{}' is unsupported; expected 0.1.0",
                    manifest.api_version
                ),
            )
            .with_path("extension.toml"),
        );
    }

    let min_shilpo = &manifest.min_shilpo_version;
    if let Ok(current_ver) = semver::Version::parse(CURRENT_SHILPO_VERSION)
        && min_shilpo > &current_ver
    {
        diagnostics.push(
            LintDiagnostic::error(
                "manifest.incompatible-shilpo-version",
                format!(
                    "min_shilpo_version '{}' is newer than current running Shilpo version '{}'",
                    min_shilpo, CURRENT_SHILPO_VERSION
                ),
            )
            .with_path("extension.toml"),
        );
    }

    // 3. Contributions Validation
    validate_contributions(&manifest, &mut diagnostics);

    // 4. Capabilities and Subscriptions Validation
    validate_capabilities_and_subscriptions(&manifest, &mut diagnostics);

    // 5. Project Configuration (shilpo-ext.json)
    validate_project_config(&root, &manifest, &mut diagnostics);

    // 6. Filesystem, Assets, and Package Limits
    let mut files = BTreeSet::new();
    files.insert(PathBuf::from("extension.toml"));
    let mut total_size = file_size_safe(&manifest_path);

    // Validate settings schemas
    for page in &manifest.contributions.settings_pages {
        let relative = PathBuf::from(&page.schema);
        let schema_path = root.join(&relative);
        if let Err(escape_diag) = check_path_containment(&root, &relative, &page.schema) {
            diagnostics.push(escape_diag);
            continue;
        }
        if !schema_path.exists() {
            diagnostics.push(
                LintDiagnostic::error(
                    "settings.missing",
                    format!(
                        "referenced settings schema '{}' does not exist",
                        page.schema
                    ),
                )
                .with_path(page.schema.clone())
                .with_remediation("Create the referenced JSON settings schema file"),
            );
            continue;
        }
        let meta = match fs::symlink_metadata(&schema_path) {
            Ok(m) => m,
            Err(e) => {
                diagnostics.push(
                    LintDiagnostic::error(
                        "settings.missing",
                        format!("failed to inspect settings schema '{}': {e}", page.schema),
                    )
                    .with_path(page.schema.clone()),
                );
                continue;
            }
        };
        if meta.file_type().is_symlink() || !meta.is_file() {
            diagnostics.push(
                LintDiagnostic::error(
                    "settings.not-file",
                    format!("settings schema '{}' is not a regular file", page.schema),
                )
                .with_path(page.schema.clone()),
            );
            continue;
        }
        validate_settings_schema_file(&schema_path, &page.schema, &mut diagnostics);
        total_size += meta.len();
        files.insert(relative);
    }

    // Optional README & LICENSE
    for optional in ["README.md", "LICENSE"] {
        let relative = PathBuf::from(optional);
        let opt_path = root.join(&relative);
        if let Ok(meta) = fs::symlink_metadata(&opt_path) {
            if meta.file_type().is_symlink() {
                diagnostics.push(
                    LintDiagnostic::error(
                        "file.type",
                        format!("packaged file '{optional}' must be a regular file, not a symlink"),
                    )
                    .with_path(optional.to_string()),
                );
            } else if meta.is_file() {
                total_size += meta.len();
                files.insert(relative);
            }
        }
    }

    // Traverse assets and i18n
    let mut visited_dirs = HashSet::new();
    for directory in ["assets", "i18n"] {
        let relative = PathBuf::from(directory);
        if matches!(
            fs::symlink_metadata(root.join(&relative)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ) {
            continue;
        }
        collect_and_validate_runtime_files(
            &root,
            &relative,
            &mut files,
            &mut total_size,
            &mut visited_dirs,
            &mut diagnostics,
        );
    }

    // Package size check
    // 7. WebAssembly Artifact Inspection
    if let Some(library) = &manifest.library {
        let relative = PathBuf::from(&library.path);
        let wasm_path = root.join(&relative);

        if let Err(escape_diag) = check_path_containment(&root, &relative, &library.path) {
            diagnostics.push(escape_diag);
        } else if !wasm_path.exists() {
            match policy {
                InspectionPolicy::Lint { .. } => {
                    diagnostics.push(
                        LintDiagnostic::info(
                            "wasm.not-built",
                            format!(
                                "WebAssembly component '{}' is not built yet; run 'shilpo ext build' before distribution",
                                library.path
                            ),
                        )
                        .with_path(library.path.clone())
                        .with_remediation("Run 'shilpo ext build' to compile the component"),
                    );
                }
                InspectionPolicy::Check => {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "file.missing",
                            format!(
                                "referenced library {} is unavailable: No such file or directory",
                                library.path
                            ),
                        )
                        .with_path(library.path.clone()),
                    );
                }
            }
        } else {
            let meta = match fs::symlink_metadata(&wasm_path) {
                Ok(m) => m,
                Err(e) => {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "file.missing",
                            format!("failed to inspect library '{}': {e}", library.path),
                        )
                        .with_path(library.path.clone()),
                    );
                    return (
                        Some(manifest),
                        files.into_iter().collect(),
                        LintReport::from_diagnostics(
                            project_display,
                            extension_id,
                            diagnostics,
                            deny_warnings,
                        ),
                    );
                }
            };

            if meta.file_type().is_symlink() || !meta.is_file() {
                diagnostics.push(
                    LintDiagnostic::error(
                        "file.type",
                        format!(
                            "referenced library '{}' is not a regular file",
                            library.path
                        ),
                    )
                    .with_path(library.path.clone()),
                );
            } else {
                let file_len = meta.len();
                total_size += file_len;
                files.insert(relative);

                if file_len > WasmRuntime::MAX_VALIDATION_COMPONENT_SIZE as u64 {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "wasm.oversized",
                            format!(
                                "WebAssembly component size ({} bytes) exceeds maximum supported validation limit of {} bytes",
                                file_len, WasmRuntime::MAX_VALIDATION_COMPONENT_SIZE
                            ),
                        )
                        .with_path(library.path.clone()),
                    );
                } else {
                    if file_len > WASM_LARGE_THRESHOLD_BYTES {
                        diagnostics.push(
                            LintDiagnostic::warning(
                                "wasm.large-artifact",
                                format!(
                                    "WebAssembly component size ({:.1} MiB) exceeds recommended 20 MiB threshold",
                                    file_len as f64 / (1024.0 * 1024.0)
                                ),
                            )
                            .with_path(library.path.clone()),
                        );
                    }

                    match fs::read(&wasm_path) {
                        Ok(bytes) => {
                            if let Err(error) =
                                WasmRuntime::validate_module_timeout(&bytes, validation_timeout)
                            {
                                let rule_id = if error.kind() == crate::RuntimeFailureKind::Timeout
                                {
                                    "wasm.timeout"
                                } else {
                                    "wasm.invalid"
                                };
                                diagnostics.push(
                                    LintDiagnostic::error(rule_id, error.to_string())
                                        .with_path(library.path.clone()),
                                );
                            } else {
                                diagnostics.push(
                                    LintDiagnostic::info(
                                        "wasm.valid",
                                        format!(
                                            "component interface validated at {}",
                                            library.path
                                        ),
                                    )
                                    .with_path(library.path.clone()),
                                );
                            }
                        }
                        Err(error) => {
                            diagnostics.push(
                                LintDiagnostic::error(
                                    "wasm.read",
                                    format!("failed to read {}: {error}", library.path),
                                )
                                .with_path(library.path.clone()),
                            );
                        }
                    }
                }
            }
        }
    }

    // Package size check
    if total_size > MAX_PACKAGE_BYTES {
        diagnostics.push(LintDiagnostic::error(
            "package.size-exceeded",
            format!(
                "total runtime package size ({} bytes) exceeds maximum limit of {} bytes",
                total_size, MAX_PACKAGE_BYTES
            ),
        ));
    }

    let report =
        LintReport::from_diagnostics(project_display, extension_id, diagnostics, deny_warnings);
    (Some(manifest), files.into_iter().collect(), report)
}

fn validate_contributions(manifest: &ExtensionManifest, diagnostics: &mut Vec<LintDiagnostic>) {
    let contribs = &manifest.contributions;

    // Check duplicate IDs per family
    check_duplicate_ids(
        &contribs
            .bar_widgets
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "bar_widgets",
        diagnostics,
    );
    check_duplicate_ids(
        &contribs
            .bar_menus
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "bar_menus",
        diagnostics,
    );
    check_duplicate_ids(
        &contribs
            .desktop_widgets
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "desktop_widgets",
        diagnostics,
    );
    check_duplicate_ids(
        &contribs
            .settings_pages
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "settings_pages",
        diagnostics,
    );
    check_duplicate_ids(
        &contribs
            .side_panels
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "side_panels",
        diagnostics,
    );
    check_duplicate_ids(
        &contribs
            .search_providers
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "search_providers",
        diagnostics,
    );
    check_duplicate_ids(
        &contribs
            .actions
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "actions",
        diagnostics,
    );
    check_duplicate_ids(
        &contribs
            .keyboard_shortcuts
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "keyboard_shortcuts",
        diagnostics,
    );
    check_duplicate_ids(
        &contribs
            .background_tasks
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "background_tasks",
        diagnostics,
    );
    check_duplicate_ids(
        &contribs
            .wallpaper_providers
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        "wallpaper_providers",
        diagnostics,
    );

    // Relationship validations
    let bar_widget_ids: HashSet<&str> =
        contribs.bar_widgets.iter().map(|c| c.id.as_str()).collect();
    for menu in &contribs.bar_menus {
        if !bar_widget_ids.contains(menu.bar_widget.as_str()) {
            diagnostics.push(
                LintDiagnostic::error(
                    "contribution.invalid-reference",
                    format!(
                        "bar_menu '{}' references unknown bar_widget '{}'",
                        menu.id, menu.bar_widget
                    ),
                )
                .with_path("extension.toml")
                .with_remediation(format!(
                    "Declare bar_widget '{}' in contributions.bar_widgets",
                    menu.bar_widget
                )),
            );
        }
    }

    let action_ids: HashSet<&str> = contribs.actions.iter().map(|c| c.id.as_str()).collect();
    for shortcut in &contribs.keyboard_shortcuts {
        if !action_ids.contains(shortcut.action.as_str()) {
            diagnostics.push(
                LintDiagnostic::error(
                    "contribution.invalid-reference",
                    format!(
                        "keyboard_shortcut '{}' references unknown action '{}'",
                        shortcut.id, shortcut.action
                    ),
                )
                .with_path("extension.toml")
                .with_remediation(format!(
                    "Declare action '{}' in contributions.actions",
                    shortcut.action
                )),
            );
        }
    }

    // Bounds validation for desktop widgets
    for widget in &contribs.desktop_widgets {
        if let (Some(min_w), Some(def_w)) = (widget.min_width, widget.default_width)
            && min_w > def_w
        {
            diagnostics.push(
                LintDiagnostic::error(
                    "contribution.invalid-bounds",
                    format!(
                        "desktop_widget '{}' min_width ({min_w}) exceeds default_width ({def_w})",
                        widget.id
                    ),
                )
                .with_path("extension.toml"),
            );
        }
        if let (Some(min_h), Some(def_h)) = (widget.min_height, widget.default_height)
            && min_h > def_h
        {
            diagnostics.push(
                LintDiagnostic::error(
                    "contribution.invalid-bounds",
                    format!(
                        "desktop_widget '{}' min_height ({min_h}) exceeds default_height ({def_h})",
                        widget.id
                    ),
                )
                .with_path("extension.toml"),
            );
        }
    }
}

fn check_duplicate_ids(ids: &[&str], family: &str, diagnostics: &mut Vec<LintDiagnostic>) {
    let mut seen = HashSet::new();
    for &id in ids {
        if !seen.insert(id) {
            diagnostics.push(
                LintDiagnostic::error(
                    "contribution.duplicate-id",
                    format!("duplicate contribution ID '{id}' declared in contributions.{family}"),
                )
                .with_path("extension.toml")
                .with_remediation(format!("Ensure all {family} have unique IDs")),
            );
        }
    }
}

fn validate_capabilities_and_subscriptions(
    manifest: &ExtensionManifest,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let mut seen_capabilities = HashSet::new();
    let mut granted_events = HashSet::new();

    for cap in &manifest.capabilities {
        let serialized = serde_json::to_string(cap).unwrap_or_default();
        if !seen_capabilities.insert(serialized) {
            diagnostics.push(
                LintDiagnostic::error(
                    "capability.duplicate",
                    format!("duplicate capability declaration '{:?}'", cap.kind()),
                )
                .with_path("extension.toml"),
            );
        }

        match cap {
            Capability::EventsSubscribe { events } => {
                if events.is_empty() {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "capability.empty-scope",
                            "events:subscribe capability declares an empty events list",
                        )
                        .with_path("extension.toml"),
                    );
                }
                for ev in events {
                    granted_events.insert(*ev);
                }
            }
            Capability::ActionsInvoke { actions } => {
                if actions.is_empty() {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "capability.empty-scope",
                            "actions:invoke capability declares an empty actions list",
                        )
                        .with_path("extension.toml"),
                    );
                }
                if actions.iter().any(|a| a == "*") {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "capability.invalid-wildcard",
                            "wildcard scope '*' is not permitted for actions:invoke",
                        )
                        .with_path("extension.toml"),
                    );
                }
            }
            Capability::NetworkHttp { hosts, paths } => {
                if hosts.is_empty() {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "capability.empty-scope",
                            "network:http capability declares an empty hosts list",
                        )
                        .with_path("extension.toml"),
                    );
                }
                if hosts
                    .iter()
                    .any(|h| h == "*" || h == "*.*" || h.starts_with("*."))
                {
                    diagnostics.push(
                        LintDiagnostic::warning(
                            "capability.broad-network-scope",
                            "network:http capability uses broad wildcard host scope; consider specifying exact domain names",
                        )
                        .with_path("extension.toml")
                        .with_remediation("Narrow host scope to explicit domain names"),
                    );
                }
                if paths.iter().any(|p| p == "*" || p == "/*") {
                    diagnostics.push(
                        LintDiagnostic::warning(
                            "capability.broad-network-scope",
                            "network:http capability uses unrestricted path wildcard; consider narrowing endpoints",
                        )
                        .with_path("extension.toml"),
                    );
                }
            }
            Capability::FilesystemRead { paths } => {
                if paths.is_empty() {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "capability.empty-scope",
                            "filesystem:read capability declares an empty paths list",
                        )
                        .with_path("extension.toml"),
                    );
                }
                for p in paths {
                    if p == "/" || p == "*" || p == "assets" || p == "data" || p == "user" {
                        diagnostics.push(
                            LintDiagnostic::warning(
                                "capability.broad-filesystem-scope",
                                format!("filesystem:read capability covers broad virtual root '{p}'; consider narrowing to subpaths"),
                            )
                            .with_path("extension.toml"),
                        );
                    }
                }
            }
            Capability::FilesystemWrite { paths } => {
                if paths.is_empty() {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "capability.empty-scope",
                            "filesystem:write capability declares an empty paths list",
                        )
                        .with_path("extension.toml"),
                    );
                }
                for p in paths {
                    if p == "/" || p == "*" || p == "assets" || p == "data" || p == "user" {
                        diagnostics.push(
                            LintDiagnostic::warning(
                                "capability.broad-filesystem-scope",
                                format!("filesystem:write capability covers broad virtual root '{p}'; consider narrowing to subpaths"),
                            )
                            .with_path("extension.toml"),
                        );
                    }
                }
            }
            Capability::Secrets { purposes } if purposes.iter().any(|p| p.as_str() == "*") => {
                diagnostics.push(
                    LintDiagnostic::error(
                        "capability.invalid-wildcard",
                        "wildcard secret purpose '*' is not permitted",
                    )
                    .with_path("extension.toml"),
                );
            }
            _ => {}
        }
    }

    // Verify all subscriptions are covered by events:subscribe
    for sub in &manifest.subscriptions {
        if !granted_events.contains(&sub.event) {
            diagnostics.push(
                LintDiagnostic::error(
                    "capability.missing-subscription-grant",
                    format!(
                        "subscription to event '{:?}' requires matching 'events:subscribe' capability",
                        sub.event
                    ),
                )
                .with_path("extension.toml")
                .with_remediation(format!("Add '{:?}' to events:subscribe capability", sub.event)),
            );
        }
    }
}

fn validate_project_config(
    root: &Path,
    manifest: &ExtensionManifest,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let config_path = root.join("shilpo-ext.json");
    if config_path.exists() {
        let meta = match fs::symlink_metadata(&config_path) {
            Ok(m) => m,
            Err(e) => {
                diagnostics.push(
                    LintDiagnostic::error(
                        "config.invalid",
                        format!("failed to inspect shilpo-ext.json: {e}"),
                    )
                    .with_path("shilpo-ext.json"),
                );
                return;
            }
        };
        if meta.file_type().is_symlink() || !meta.is_file() {
            diagnostics.push(
                LintDiagnostic::error("config.not-file", "shilpo-ext.json is not a regular file")
                    .with_path("shilpo-ext.json"),
            );
            return;
        }

        let source = match fs::read_to_string(&config_path) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(
                    LintDiagnostic::error(
                        "config.read",
                        format!("failed to read shilpo-ext.json: {e}"),
                    )
                    .with_path("shilpo-ext.json"),
                );
                return;
            }
        };

        let config: ExtensionProjectConfig = match serde_json::from_str(&source) {
            Ok(cfg) => cfg,
            Err(e) => {
                diagnostics.push(
                    LintDiagnostic::error(
                        "config.invalid-json",
                        format!("shilpo-ext.json is not valid JSON: {e}"),
                    )
                    .with_path("shilpo-ext.json"),
                );
                return;
            }
        };

        // Validate entry path if present
        if let Some(entry) = &config.entry
            && let Err(diagnostic) = validate_config_path(root, entry, "config.entry", false)
        {
            diagnostics.push(diagnostic);
        }

        // Validate language consistency
        match config.language {
            ExtensionLanguage::Rust => {
                let crate_root = config.crate_dir.as_deref().unwrap_or(".");
                if let Err(diagnostic) =
                    validate_config_path(root, crate_root, "config.crate", true)
                {
                    diagnostics.push(diagnostic);
                } else {
                    let cargo_path = root.join(crate_root).join("Cargo.toml");
                    if !cargo_path.is_file()
                        || fs::symlink_metadata(&cargo_path)
                            .is_ok_and(|metadata| metadata.file_type().is_symlink())
                    {
                        diagnostics.push(
                            LintDiagnostic::error(
                                "config.language-mismatch",
                                "shilpo-ext.json declares Rust language, but Cargo.toml was not found as a regular file",
                            )
                            .with_path("shilpo-ext.json"),
                        );
                    }
                }
            }
            ExtensionLanguage::Typescript => {
                let has_pkg = root.join("package.json").exists();
                let has_tsconfig = root.join("tsconfig.json").exists();
                let has_deno = root.join("deno.json").exists();
                if !has_pkg && !has_tsconfig && !has_deno {
                    diagnostics.push(
                        LintDiagnostic::error(
                            "config.language-mismatch",
                            "shilpo-ext.json declares TypeScript language, but package.json, tsconfig.json, or deno.json is missing",
                        )
                        .with_path("shilpo-ext.json"),
                    );
                }
            }
        }
    } else {
        // Discovery ambiguity check
        let has_cargo = root.join("Cargo.toml").exists();
        let has_ts = root.join("package.json").exists() || root.join("tsconfig.json").exists();
        if has_cargo && has_ts {
            diagnostics.push(
                LintDiagnostic::warning(
                    "config.ambiguous",
                    "project directory contains both Cargo.toml and package.json/tsconfig.json; consider adding shilpo-ext.json to specify language",
                )
                .with_remediation("Add a shilpo-ext.json to explicitly set the extension language"),
            );
        }
    }

    if let Some(library) = &manifest.library
        && let Err(diagnostic) =
            check_path_containment(root, Path::new(&library.path), &library.path)
    {
        diagnostics.push(diagnostic);
    }
}

fn validate_config_path(
    root: &Path,
    value: &str,
    label: &str,
    expect_directory: bool,
) -> Result<PathBuf, LintDiagnostic> {
    let relative = Path::new(value);
    if value.trim().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(LintDiagnostic::error(
            label,
            format!("path '{value}' must be a safe relative path"),
        )
        .with_path("shilpo-ext.json"));
    }

    let path = root.join(relative);
    let canonical = path.canonicalize().map_err(|error| {
        LintDiagnostic::error(
            if expect_directory {
                "config.crate-missing"
            } else {
                "config.entry-missing"
            },
            format!("declared path '{value}' does not exist: {error}"),
        )
        .with_path("shilpo-ext.json")
    })?;
    if !canonical.starts_with(root) {
        return Err(LintDiagnostic::error(
            "config.path-escape",
            format!("declared path '{value}' resolves outside the project root"),
        )
        .with_path("shilpo-ext.json"));
    }

    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        LintDiagnostic::error(label, format!("failed to inspect '{value}': {error}"))
            .with_path("shilpo-ext.json")
    })?;
    if metadata.file_type().is_symlink()
        || (expect_directory && !metadata.is_dir())
        || (!expect_directory && !metadata.is_file())
    {
        return Err(LintDiagnostic::error(
            "config.not-regular",
            format!("declared path '{value}' has the wrong file type"),
        )
        .with_path("shilpo-ext.json"));
    }
    Ok(path)
}

fn check_path_containment(root: &Path, relative: &Path, label: &str) -> Result<(), LintDiagnostic> {
    for comp in relative.components() {
        match comp {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(LintDiagnostic::error(
                    "asset.symlink-escape",
                    format!("referenced path '{label}' attempts to escape the project root"),
                )
                .with_path(label.to_string()));
            }
            _ => {}
        }
    }
    let target = root.join(relative);
    if let Ok(canonical) = target.canonicalize()
        && !canonical.starts_with(root)
    {
        return Err(LintDiagnostic::error(
            "asset.symlink-escape",
            format!("path '{label}' resolves outside project root"),
        )
        .with_path(label.to_string()));
    }
    Ok(())
}

fn validate_settings_schema_file(
    path: &Path,
    relative_str: &str,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            diagnostics.push(
                LintDiagnostic::error(
                    "settings.read",
                    format!("failed to read settings schema '{relative_str}': {e}"),
                )
                .with_path(relative_str.to_string()),
            );
            return;
        }
    };

    let schema: Value = match serde_json::from_str(&source) {
        Ok(s) => s,
        Err(e) => {
            diagnostics.push(
                LintDiagnostic::error(
                    "settings.invalid-json",
                    format!("settings schema '{relative_str}' is invalid JSON: {e}"),
                )
                .with_path(relative_str.to_string()),
            );
            return;
        }
    };

    if let Err(e) = jsonschema::meta::validate(&schema) {
        diagnostics.push(
            LintDiagnostic::error(
                "settings.invalid-schema",
                format!("settings schema '{relative_str}' is not valid JSON Schema: {e}"),
            )
            .with_path(relative_str.to_string()),
        );
        return;
    }

    if contains_remote_reference(&schema) {
        diagnostics.push(
            LintDiagnostic::error(
                "settings.remote-reference",
                format!("settings schema '{relative_str}' contains remote $ref reference"),
            )
            .with_path(relative_str.to_string())
            .with_remediation("Use local inline definitions instead of remote URLs in $ref"),
        );
        return;
    }

    let defaults = extract_settings_defaults(&schema);
    match jsonschema::validator_for(&schema) {
        Ok(validator) => {
            if let Err(e) = validator.validate(&defaults) {
                diagnostics.push(
                    LintDiagnostic::error(
                        "settings.invalid-defaults",
                        format!("defaults in settings schema '{relative_str}' are invalid: {e}"),
                    )
                    .with_path(relative_str.to_string())
                    .with_remediation(
                        "Ensure all default values conform to the schema definitions",
                    ),
                );
            } else {
                diagnostics.push(
                    LintDiagnostic::info(
                        "settings.valid",
                        format!("schema and defaults validated at {relative_str}"),
                    )
                    .with_path(relative_str.to_string()),
                );
            }
        }
        Err(e) => {
            diagnostics.push(
                LintDiagnostic::error(
                    "settings.invalid-schema",
                    format!("failed to compile settings validator for '{relative_str}': {e}"),
                )
                .with_path(relative_str.to_string()),
            );
        }
    }
}

fn extract_settings_defaults(schema: &Value) -> Value {
    let mut defaults = serde_json::Map::new();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (key, prop) in properties {
            if let Some(val) = prop.get("default") {
                defaults.insert(key.clone(), val.clone());
            }
        }
    }
    Value::Object(defaults)
}

fn contains_remote_reference(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(k, v)| {
            (k == "$ref" && v.as_str().is_some_and(|r| r.contains("://")))
                || contains_remote_reference(v)
        }),
        Value::Array(arr) => arr.iter().any(contains_remote_reference),
        _ => false,
    }
}

fn collect_and_validate_runtime_files(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
    total_size: &mut u64,
    visited_dirs: &mut HashSet<PathBuf>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let path = root.join(relative);
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) => {
            let rule = if e.kind() == std::io::ErrorKind::NotFound {
                "asset.missing"
            } else {
                "asset.read"
            };
            diagnostics.push(
                LintDiagnostic::error(
                    rule,
                    format!("failed to inspect asset '{}': {e}", relative.display()),
                )
                .with_path(relative.to_string_lossy().to_string()),
            );
            return;
        }
    };

    if meta.file_type().is_symlink() {
        let target = match path.canonicalize() {
            Ok(t) => t,
            Err(_) => {
                diagnostics.push(
                    LintDiagnostic::error(
                        "asset.symlink-escape",
                        format!("broken symbolic link '{}'", relative.display()),
                    )
                    .with_path(relative.to_string_lossy().to_string()),
                );
                return;
            }
        };
        if !target.starts_with(root) {
            diagnostics.push(
                LintDiagnostic::error(
                    "asset.symlink-escape",
                    format!(
                        "symbolic link '{}' escapes project root",
                        relative.display()
                    ),
                )
                .with_path(relative.to_string_lossy().to_string()),
            );
            return;
        }
        diagnostics.push(
            LintDiagnostic::error(
                "asset.symlink",
                format!(
                    "symbolic links are not packageable in extensions: {}",
                    relative.display()
                ),
            )
            .with_path(relative.to_string_lossy().to_string()),
        );
        return;
    }

    if meta.is_file() {
        let file_len = meta.len();
        *total_size += file_len;
        if file_len > MAX_FILE_BYTES {
            diagnostics.push(
                LintDiagnostic::error(
                    "file.size-exceeded",
                    format!(
                        "file '{}' ({file_len} bytes) exceeds the per-file limit of {MAX_FILE_BYTES} bytes",
                        relative.display()
                    ),
                )
                .with_path(relative.to_string_lossy().to_string()),
            );
        }
        files.insert(relative.to_path_buf());

        // Deep asset validation for PNG and SVG
        if let Some(ext) = relative.extension().and_then(|s| s.to_str())
            && file_len <= MAX_FILE_BYTES
        {
            let lower = ext.to_lowercase();
            if lower == "png" || lower == "svg" {
                match fs::read(&path) {
                    Ok(bytes) => {
                        let result = if lower == "png" {
                            validate_png_bytes(&bytes)
                        } else {
                            validate_svg_bytes(&bytes)
                        };
                        if let Err(err) = result {
                            diagnostics.push(
                                LintDiagnostic::error(
                                    if lower == "png" {
                                        "asset.invalid-png"
                                    } else {
                                        "asset.invalid-svg"
                                    },
                                    format!(
                                        "asset '{}' is not a valid {} image: {err}",
                                        relative.display(),
                                        lower
                                    ),
                                )
                                .with_path(relative.to_string_lossy().to_string()),
                            );
                        }
                    }
                    Err(error) => diagnostics.push(
                        LintDiagnostic::error(
                            "asset.read",
                            format!("failed to read asset '{}': {error}", relative.display()),
                        )
                        .with_path(relative.to_string_lossy().to_string()),
                    ),
                }
            }
        }
        return;
    }

    if !meta.is_dir() {
        diagnostics.push(
            LintDiagnostic::error(
                "asset.special-file",
                format!("special file '{}' is not packageable", relative.display()),
            )
            .with_path(relative.to_string_lossy().to_string()),
        );
        return;
    }

    // Directory recursion with cycle detection
    let canonical_dir = match path.canonicalize() {
        Ok(c) => c,
        Err(error) => {
            diagnostics.push(
                LintDiagnostic::error(
                    "asset.read",
                    format!(
                        "failed to resolve asset directory '{}': {error}",
                        relative.display()
                    ),
                )
                .with_path(relative.to_string_lossy().to_string()),
            );
            return;
        }
    };
    if !visited_dirs.insert(canonical_dir) {
        diagnostics.push(
            LintDiagnostic::error(
                "asset.cycle",
                format!("directory cycle detected at '{}'", relative.display()),
            )
            .with_path(relative.to_string_lossy().to_string()),
        );
        return;
    }

    let entries = match fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(
                LintDiagnostic::error(
                    "asset.read",
                    format!(
                        "failed to read asset directory '{}': {error}",
                        relative.display()
                    ),
                )
                .with_path(relative.to_string_lossy().to_string()),
            );
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let child_rel = relative.join(entry.file_name());
                collect_and_validate_runtime_files(
                    root,
                    &child_rel,
                    files,
                    total_size,
                    visited_dirs,
                    diagnostics,
                );
            }
            Err(error) => diagnostics.push(
                LintDiagnostic::error(
                    "asset.read",
                    format!(
                        "failed to enumerate asset directory '{}': {error}",
                        relative.display()
                    ),
                )
                .with_path(relative.to_string_lossy().to_string()),
            ),
        }
    }
}

pub fn validate_png_bytes(bytes: &[u8]) -> Result<(), String> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 8 || &bytes[..8] != PNG_SIGNATURE {
        return Err("invalid PNG signature".to_string());
    }
    if bytes.len() < 8 + 12 + 13 {
        return Err("truncated PNG header".to_string());
    }
    let mut offset = 8;
    let mut saw_ihdr = false;
    let mut saw_iend = false;

    while offset + 8 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        offset += 8;

        if !saw_ihdr {
            if chunk_type != b"IHDR" || length != 13 {
                return Err("first PNG chunk must be IHDR of length 13".to_string());
            }
            if offset + 13 + 4 > bytes.len() {
                return Err("truncated IHDR chunk".to_string());
            }
            let width = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
            let height = u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
            if width == 0 || height == 0 {
                return Err(
                    "invalid image dimensions in IHDR (width and height must be positive)"
                        .to_string(),
                );
            }
            saw_ihdr = true;
        }

        if chunk_type == b"IEND" {
            saw_iend = true;
            break;
        }

        let total_chunk_advance = match length.checked_add(4) {
            Some(adv) => adv,
            None => return Err("corrupted PNG chunk length".to_string()),
        };
        if offset + total_chunk_advance > bytes.len() {
            return Err("truncated PNG chunk data".to_string());
        }
        offset += total_chunk_advance;
    }

    if !saw_ihdr {
        return Err("missing IHDR chunk".to_string());
    }
    if !saw_iend {
        return Err("missing IEND chunk".to_string());
    }
    Ok(())
}

pub fn validate_svg_bytes(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("SVG file exceeds maximum allowed size".to_string());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "SVG file is not valid UTF-8".to_string())?;

    let mut pos = 0;
    let len = text.len();
    let mut root_tag_name: Option<String> = None;

    while pos < len {
        if text[pos..].starts_with("<!--") {
            pos += 4;
            while pos + 3 <= len && !text[pos..].starts_with("-->") {
                pos += 1;
            }
            pos = (pos + 3).min(len);
            continue;
        }
        if text[pos..].starts_with("<?xml") {
            pos += 5;
            while pos + 2 <= len && !text[pos..].starts_with("?>") {
                pos += 1;
            }
            pos = (pos + 2).min(len);
            continue;
        }
        if text[pos..].starts_with("<!DOCTYPE") || text[pos..].starts_with("<!doctype") {
            pos += 9;
            let mut depth = 0;
            while pos < len {
                if text[pos..].starts_with('[') {
                    depth += 1;
                } else if text[pos..].starts_with(']') {
                    depth -= 1;
                } else if text[pos..].starts_with('>') && depth == 0 {
                    pos += 1;
                    break;
                }
                pos += 1;
            }
            continue;
        }
        if text[pos..].starts_with('<') {
            let tag_start = pos + 1;
            if tag_start < len {
                let first_char = text.as_bytes()[tag_start];
                if first_char != b'/' && first_char != b'!' && first_char != b'?' {
                    let name_end = text[tag_start..]
                        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                        .map(|offset| tag_start + offset)
                        .unwrap_or(len);
                    let tag_name = &text[tag_start..name_end];
                    root_tag_name = Some(tag_name.to_lowercase());
                    break;
                }
            }
        }
        pos += 1;
    }

    match root_tag_name.as_deref() {
        Some("svg") => Ok(()),
        Some(other) => Err(format!(
            "expected root element to be <svg>, found <{other}>"
        )),
        None => Err("missing root <svg> element in SVG document".to_string()),
    }
}

fn file_size_safe(path: &Path) -> u64 {
    fs::symlink_metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn line_col_from_offset(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
