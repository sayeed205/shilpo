//! Extension scaffolding engine.
//!
//! Provides deterministic, atomic, and safe generation of Rust and TypeScript extension projects.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use semver::Version;
use serde::{Deserialize, Serialize};
use shilpo_ext_api::{
    ActionContribution, BarWidgetContribution, Capability, ContributionId, Contributions,
    DesktopWidgetContribution, EventKind, ExtensionId, ExtensionManifest, LibraryConfig,
    SUPPORTED_API_VERSION, SUPPORTED_SCHEMA_VERSION, SettingsPageContribution,
    SidePanelContribution, Subscription,
};

use crate::build::{
    ExtensionLanguage, ExtensionProjectConfig, PROCESS_CANCELLED, ProcessCommand, ProcessRunner,
    build_extension,
};

static SCAFFOLD_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Supported programming languages for extension projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StarterLanguage {
    Rust,
    Typescript,
}

impl std::fmt::Display for StarterLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust => write!(f, "rust"),
            Self::Typescript => write!(f, "typescript"),
        }
    }
}

impl std::str::FromStr for StarterLanguage {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rust" => Ok(Self::Rust),
            "typescript" | "ts" => Ok(Self::Typescript),
            other => Err(format!(
                "unsupported language '{other}': expected 'rust' or 'typescript'"
            )),
        }
    }
}

/// Six canonical starter contribution kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StarterContribution {
    BarWidget,
    DesktopWidget,
    SettingsPage,
    SidePanel,
    Action,
    Empty,
}

impl std::fmt::Display for StarterContribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BarWidget => write!(f, "bar-widget"),
            Self::DesktopWidget => write!(f, "desktop-widget"),
            Self::SettingsPage => write!(f, "settings-page"),
            Self::SidePanel => write!(f, "side-panel"),
            Self::Action => write!(f, "action"),
            Self::Empty => write!(f, "empty"),
        }
    }
}

impl std::str::FromStr for StarterContribution {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bar-widget" | "bar_widget" | "bar" => Ok(Self::BarWidget),
            "desktop-widget" | "desktop_widget" | "desktop" => Ok(Self::DesktopWidget),
            "settings-page" | "settings_page" | "settings" => Ok(Self::SettingsPage),
            "side-panel" | "side_panel" | "panel" => Ok(Self::SidePanel),
            "action" => Ok(Self::Action),
            "empty" | "none" => Ok(Self::Empty),
            other => Err(format!(
                "unsupported starter contribution '{other}': expected 'bar-widget', 'desktop-widget', 'settings-page', 'side-panel', 'action', or 'empty'"
            )),
        }
    }
}

/// Supported package managers for TypeScript extensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    #[default]
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }
}

impl std::fmt::Display for PackageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for PackageManager {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "npm" => Ok(Self::Npm),
            "pnpm" => Ok(Self::Pnpm),
            "yarn" => Ok(Self::Yarn),
            "bun" => Ok(Self::Bun),
            other => Err(format!(
                "unsupported package manager '{other}': expected 'npm', 'pnpm', 'yarn', or 'bun'"
            )),
        }
    }
}

/// Scaffolding options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaffoldOptions {
    pub name: String,
    pub target_dir: PathBuf,
    pub language: StarterLanguage,
    pub contribution: StarterContribution,
    pub package_manager: Option<PackageManager>,
    pub extension_id: Option<String>,
    pub package_name: Option<String>,
    pub description: Option<String>,
    pub capabilities: Vec<Capability>,
    pub subscriptions: Vec<Subscription>,
    pub install: bool,
    pub build: bool,
    pub git: bool,
}

/// Scaffolding outcome result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaffoldResult {
    pub name: String,
    pub extension_id: ExtensionId,
    pub language: StarterLanguage,
    pub contribution: StarterContribution,
    pub target_dir: PathBuf,
    pub files_created: Vec<String>,
    pub installed: bool,
    pub built: bool,
    pub git_initialized: bool,
    pub next_steps: Vec<String>,
}

/// Scaffolder error types.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ScaffoldError {
    InvalidTarget(String),
    InvalidExtensionId(String),
    InvalidPackageName(String),
    InvalidCapability(String),
    InvalidSubscription(String),
    CapabilityConflict(String),
    TargetExistsAndNotEmpty(PathBuf),
    TargetIsFile(PathBuf),
    TargetIsSymlink(PathBuf),
    PathTraversal(PathBuf),
    StagingFailed(String),
    ActivationFailed(String),
    StageFailed {
        stage: String,
        recovery_command: String,
        message: String,
    },
    Cancelled,
    Io(String),
}

impl std::fmt::Display for ScaffoldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTarget(msg) => write!(f, "invalid target directory: {msg}"),
            Self::InvalidExtensionId(msg) => write!(f, "invalid extension ID: {msg}"),
            Self::InvalidPackageName(msg) => write!(f, "invalid package/crate name: {msg}"),
            Self::InvalidCapability(msg) => write!(f, "invalid capability: {msg}"),
            Self::InvalidSubscription(msg) => write!(f, "invalid subscription: {msg}"),
            Self::CapabilityConflict(msg) => write!(f, "capability conflict: {msg}"),
            Self::TargetExistsAndNotEmpty(path) => {
                write!(
                    f,
                    "target directory '{}' already exists and is not empty",
                    path.display()
                )
            }
            Self::TargetIsFile(path) => {
                write!(
                    f,
                    "target path '{}' already exists and is a file",
                    path.display()
                )
            }
            Self::TargetIsSymlink(path) => {
                write!(
                    f,
                    "target path '{}' is a symlink (symlinks are not permitted as target directories)",
                    path.display()
                )
            }
            Self::PathTraversal(path) => {
                write!(
                    f,
                    "target path '{}' contains forbidden parent directory traversal ('..')",
                    path.display()
                )
            }
            Self::StagingFailed(msg) => write!(f, "failed to generate project in staging: {msg}"),
            Self::ActivationFailed(msg) => {
                write!(f, "failed to activate generated project: {msg}")
            }
            Self::StageFailed {
                stage,
                recovery_command,
                message,
            } => {
                write!(
                    f,
                    "failed during {stage} stage: {message}\nRecovery command: {recovery_command}"
                )
            }
            Self::Cancelled => write!(f, "operation cancelled"),
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl std::error::Error for ScaffoldError {}

impl From<std::io::Error> for ScaffoldError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

/// RAII guard to clean up staging directory if generation fails or is aborted before activation.
struct StagingGuard {
    path: PathBuf,
    active: bool,
}

impl StagingGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, active: true }
    }

    fn defuse(mut self) {
        self.active = false;
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.active && self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Derives a valid `ExtensionId` from a human display name or explicit override.
pub fn derive_extension_id(
    name: &str,
    explicit: Option<&str>,
) -> Result<ExtensionId, ScaffoldError> {
    if let Some(explicit_id) = explicit {
        return ExtensionId::new(explicit_id)
            .map_err(|err| ScaffoldError::InvalidExtensionId(err.to_string()));
    }

    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ScaffoldError::InvalidExtensionId(
            "name cannot be empty".into(),
        ));
    }

    // Check if the name itself is already a reverse-domain format
    if let Ok(id) = ExtensionId::new(trimmed) {
        return Ok(id);
    }

    // Convert display name to kebab-case segment
    let mut kebab = String::new();
    let mut last_dash = true;
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() {
            kebab.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            kebab.push('-');
            last_dash = true;
        }
    }
    let kebab = kebab.trim_matches('-');
    if kebab.is_empty() {
        return Err(ScaffoldError::InvalidExtensionId(
            "name contains no valid alphanumeric characters".into(),
        ));
    }

    // Reverse-domain requires at least 3 segments (e.g. dev.local.<name>)
    let derived = format!("dev.local.{kebab}");
    ExtensionId::new(&derived).map_err(|err| ScaffoldError::InvalidExtensionId(err.to_string()))
}

/// Derives a valid package/crate name for the target language.
pub fn derive_package_name(
    name: &str,
    language: StarterLanguage,
    explicit: Option<&str>,
) -> Result<String, ScaffoldError> {
    if let Some(exp) = explicit {
        let exp = exp.trim();
        if exp.is_empty() {
            return Err(ScaffoldError::InvalidPackageName(
                "package name cannot be empty".into(),
            ));
        }
        match language {
            StarterLanguage::Rust => {
                let valid = !exp.is_empty()
                    && exp
                        .bytes()
                        .next()
                        .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
                    && exp
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
                if !valid {
                    return Err(ScaffoldError::InvalidPackageName(format!(
                        "invalid Rust crate name '{exp}': must be lowercase alphanumeric with underscores and start with a letter/underscore"
                    )));
                }
                return Ok(exp.to_string());
            }
            StarterLanguage::Typescript => {
                let valid = !exp.is_empty()
                    && exp.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                    });
                if !valid {
                    return Err(ScaffoldError::InvalidPackageName(format!(
                        "invalid npm package name '{exp}': must be lowercase alphanumeric with hyphens or underscores"
                    )));
                }
                return Ok(exp.to_string());
            }
        }
    }

    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ScaffoldError::InvalidPackageName(
            "name cannot be empty".into(),
        ));
    }

    match language {
        StarterLanguage::Rust => {
            let mut snake = String::new();
            let mut last_under = true;
            for ch in trimmed.chars() {
                if ch.is_ascii_alphanumeric() {
                    snake.push(ch.to_ascii_lowercase());
                    last_under = false;
                } else if !last_under {
                    snake.push('_');
                    last_under = true;
                }
            }
            let mut snake = snake.trim_matches('_').to_string();
            if snake.is_empty() {
                snake = "my_extension".to_string();
            } else if snake.bytes().next().is_some_and(|b| b.is_ascii_digit()) {
                snake = format!("ext_{snake}");
            }
            Ok(snake)
        }
        StarterLanguage::Typescript => {
            let mut kebab = String::new();
            let mut last_dash = true;
            for ch in trimmed.chars() {
                if ch.is_ascii_alphanumeric() {
                    kebab.push(ch.to_ascii_lowercase());
                    last_dash = false;
                } else if !last_dash {
                    kebab.push('-');
                    last_dash = true;
                }
            }
            let mut kebab = kebab.trim_matches('-').to_string();
            if kebab.is_empty() {
                kebab = "my-extension".to_string();
            }
            Ok(kebab)
        }
    }
}

/// Validates target path safety and existence constraints.
pub fn validate_target_path(target_dir: &Path) -> Result<(), ScaffoldError> {
    // Reject path traversals escaping parent
    for component in target_dir.components() {
        if let Component::ParentDir = component {
            return Err(ScaffoldError::PathTraversal(target_dir.to_path_buf()));
        }
    }

    // Check symlink
    if let Ok(meta) = fs::symlink_metadata(target_dir) {
        if meta.file_type().is_symlink() {
            return Err(ScaffoldError::TargetIsSymlink(target_dir.to_path_buf()));
        }
        if meta.is_file() {
            return Err(ScaffoldError::TargetIsFile(target_dir.to_path_buf()));
        }
        if meta.is_dir() {
            let mut entries = fs::read_dir(target_dir)
                .map_err(|e| ScaffoldError::Io(format!("failed to read target directory: {e}")))?;
            if entries.next().is_some() {
                return Err(ScaffoldError::TargetExistsAndNotEmpty(
                    target_dir.to_path_buf(),
                ));
            }
        }
    }

    Ok(())
}

/// Validates and synchronizes capabilities and event subscriptions.
pub fn synchronize_capabilities_and_subscriptions(
    capabilities: &[Capability],
    subscriptions: &[Subscription],
) -> Result<(Vec<Capability>, Vec<Subscription>), ScaffoldError> {
    let mut merged_capabilities = capabilities.to_vec();
    let mut merged_subscriptions = Vec::new();
    let mut seen_events = std::collections::BTreeSet::new();

    // Validate and deduplicate subscriptions
    for sub in subscriptions {
        if seen_events.insert(sub.event) {
            merged_subscriptions.push(sub.clone());
        }
    }

    // Validate capabilities
    for cap in &merged_capabilities {
        match cap {
            Capability::EventsSubscribe { events } if events.is_empty() => {
                return Err(ScaffoldError::InvalidCapability(
                    "'events:subscribe' capability must declare at least one event".into(),
                ));
            }
            Capability::WallpaperSet { sources } if sources.is_empty() => {
                return Err(ScaffoldError::InvalidCapability(
                    "'wallpaper:set' capability must declare at least one source".into(),
                ));
            }
            Capability::ActionsInvoke { actions }
                if actions.is_empty() || actions.iter().any(|a| a.trim().is_empty()) =>
            {
                return Err(ScaffoldError::InvalidCapability(
                    "'actions:invoke' capability must declare valid non-empty action IDs".into(),
                ));
            }
            Capability::NetworkHttp { hosts, paths: _ }
                if hosts.is_empty() || hosts.iter().any(|h| h.trim().is_empty()) =>
            {
                return Err(ScaffoldError::InvalidCapability(
                    "'network:http' capability must declare at least one non-empty host".into(),
                ));
            }
            Capability::FilesystemRead { paths }
                if paths.is_empty() || paths.iter().any(|p| p.trim().is_empty()) =>
            {
                return Err(ScaffoldError::InvalidCapability(
                    "'filesystem:read' capability must declare at least one non-empty path".into(),
                ));
            }
            Capability::FilesystemWrite { paths }
                if paths.is_empty() || paths.iter().any(|p| p.trim().is_empty()) =>
            {
                return Err(ScaffoldError::InvalidCapability(
                    "'filesystem:write' capability must declare at least one non-empty path".into(),
                ));
            }
            _ => {}
        }

        let wildcard_scope = match cap {
            Capability::ActionsInvoke { actions } => actions
                .iter()
                .any(|value| value.contains('*') || value.contains('?')),
            Capability::NetworkHttp { hosts, paths } => hosts
                .iter()
                .chain(paths.iter())
                .any(|value| value.contains('*') || value.contains('?')),
            Capability::FilesystemRead { paths } | Capability::FilesystemWrite { paths } => paths
                .iter()
                .any(|value| value.contains('*') || value.contains('?')),
            _ => false,
        };
        if wildcard_scope {
            return Err(ScaffoldError::InvalidCapability(
                "wildcard capability scopes are not permitted; provide explicit values".into(),
            ));
        }
    }

    // Synchronize event subscriptions with events:subscribe capability
    if !merged_subscriptions.is_empty() {
        let subscribed_events: Vec<EventKind> =
            merged_subscriptions.iter().map(|s| s.event).collect();
        let mut found_events_cap = false;

        for cap in &mut merged_capabilities {
            if let Capability::EventsSubscribe { events } = cap {
                found_events_cap = true;
                for sub_event in &subscribed_events {
                    if !events.contains(sub_event) {
                        return Err(ScaffoldError::CapabilityConflict(format!(
                            "subscription '{sub_event:?}' is not authorized by the explicit events:subscribe capability"
                        )));
                    }
                }
            }
        }

        if !found_events_cap {
            merged_capabilities.push(Capability::EventsSubscribe {
                events: subscribed_events,
            });
        }
    }

    // Sort capabilities deterministically by kind tag
    merged_capabilities.sort_by_key(|c| format!("{c:?}"));
    merged_subscriptions.sort_by_key(|s| format!("{s:?}"));

    Ok((merged_capabilities, merged_subscriptions))
}

/// Generates an extension project according to options and executes optional stages.
pub fn scaffold_extension<R: ProcessRunner>(
    options: &ScaffoldOptions,
    runner: &R,
) -> Result<ScaffoldResult, ScaffoldError> {
    PROCESS_CANCELLED.store(false, std::sync::atomic::Ordering::Release);
    install_cancel_handlers();
    if cancellation_requested() {
        return Err(ScaffoldError::Cancelled);
    }
    // 1. Validation of target path
    validate_target_path(&options.target_dir)?;

    if options.language == StarterLanguage::Rust && options.package_manager.is_some() {
        return Err(ScaffoldError::InvalidTarget(
            "--package-manager can only be used with TypeScript extensions".into(),
        ));
    }

    // 2. Derive identifiers
    let extension_id = derive_extension_id(&options.name, options.extension_id.as_deref())?;
    let package_name = derive_package_name(
        &options.name,
        options.language,
        options.package_name.as_deref(),
    )?;

    // 3. Synchronize capabilities and subscriptions
    let (capabilities, subscriptions) =
        synchronize_capabilities_and_subscriptions(&options.capabilities, &options.subscriptions)?;

    // 4. Staging setup
    let target_parent = options
        .target_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    if !target_parent.exists() {
        fs::create_dir_all(&target_parent).map_err(|e| {
            ScaffoldError::Io(format!(
                "failed to create parent directory '{}': {e}",
                target_parent.display()
            ))
        })?;
    }

    let counter = SCAFFOLD_COUNTER.fetch_add(1, Ordering::SeqCst);
    let staging_name = format!(
        ".{}.tmp-{}-{}-{}",
        options
            .target_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("ext"),
        std::process::id(),
        counter,
        uuid::Uuid::new_v4().simple()
    );
    let staging_dir = target_parent.join(staging_name);
    fs::create_dir_all(&staging_dir).map_err(|e| {
        ScaffoldError::StagingFailed(format!(
            "failed to create staging directory '{}': {e}",
            staging_dir.display()
        ))
    })?;

    let guard = StagingGuard::new(staging_dir.clone());

    // 5. Generate and write files into staging
    let mut files_created = Vec::new();

    // A. extension.toml
    let manifest_toml = generate_manifest(
        &options.name,
        &extension_id,
        &package_name,
        options.description.as_deref(),
        options.language,
        options.contribution,
        &capabilities,
        &subscriptions,
    )?;
    fs::write(staging_dir.join("extension.toml"), manifest_toml).map_err(|e| {
        ScaffoldError::StagingFailed(format!("failed to write extension.toml: {e}"))
    })?;
    files_created.push("extension.toml".into());

    // B. shilpo-ext.json
    let project_config_json = generate_project_config(options.language);
    fs::write(staging_dir.join("shilpo-ext.json"), project_config_json).map_err(|e| {
        ScaffoldError::StagingFailed(format!("failed to write shilpo-ext.json: {e}"))
    })?;
    files_created.push("shilpo-ext.json".into());

    // C. Language-specific files
    match options.language {
        StarterLanguage::Rust => {
            // Cargo.toml
            let cargo_toml = generate_cargo_toml(&package_name, options.description.as_deref());
            fs::write(staging_dir.join("Cargo.toml"), cargo_toml).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write Cargo.toml: {e}"))
            })?;
            files_created.push("Cargo.toml".into());

            // src/lib.rs
            let src_dir = staging_dir.join("src");
            fs::create_dir_all(&src_dir).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to create src directory: {e}"))
            })?;
            let lib_rs = generate_rust_source(&options.name, options.contribution);
            fs::write(src_dir.join("lib.rs"), lib_rs).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write src/lib.rs: {e}"))
            })?;
            files_created.push("src/lib.rs".into());

            // .gitignore
            let gitignore = "/target\n**/*.rs.bk\n";
            fs::write(staging_dir.join(".gitignore"), gitignore).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write .gitignore: {e}"))
            })?;
            files_created.push(".gitignore".into());

            // README.md
            let readme = generate_rust_readme(&options.name, options.description.as_deref());
            fs::write(staging_dir.join("README.md"), readme).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write README.md: {e}"))
            })?;
            files_created.push("README.md".into());
        }
        StarterLanguage::Typescript => {
            let pm = options.package_manager.unwrap_or(PackageManager::Npm);

            // package.json
            let package_json = generate_package_json(&package_name, options.description.as_deref());
            fs::write(staging_dir.join("package.json"), package_json).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write package.json: {e}"))
            })?;
            files_created.push("package.json".into());

            // tsconfig.json
            let tsconfig_json = generate_tsconfig_json();
            fs::write(staging_dir.join("tsconfig.json"), tsconfig_json).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write tsconfig.json: {e}"))
            })?;
            files_created.push("tsconfig.json".into());

            // .npmrc for JSR package resolution
            let npmrc = "@jsr:registry=https://npm.jsr.io\n";
            fs::write(staging_dir.join(".npmrc"), npmrc).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write .npmrc: {e}"))
            })?;
            files_created.push(".npmrc".into());

            // src/extension.ts
            let src_dir = staging_dir.join("src");
            fs::create_dir_all(&src_dir).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to create src directory: {e}"))
            })?;
            let extension_ts = generate_typescript_source(&options.name, options.contribution);
            fs::write(src_dir.join("extension.ts"), extension_ts).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write src/extension.ts: {e}"))
            })?;
            files_created.push("src/extension.ts".into());

            // .gitignore
            let gitignore = "node_modules/\ndist/\n";
            fs::write(staging_dir.join(".gitignore"), gitignore).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write .gitignore: {e}"))
            })?;
            files_created.push(".gitignore".into());

            // README.md
            let readme =
                generate_typescript_readme(&options.name, options.description.as_deref(), pm);
            fs::write(staging_dir.join("README.md"), readme).map_err(|e| {
                ScaffoldError::StagingFailed(format!("failed to write README.md: {e}"))
            })?;
            files_created.push("README.md".into());
        }
    }

    // D. settings.schema.json (if SettingsPage)
    if options.contribution == StarterContribution::SettingsPage {
        let schema_json = generate_settings_schema(&options.name);
        fs::write(staging_dir.join("settings.schema.json"), schema_json).map_err(|e| {
            ScaffoldError::StagingFailed(format!("failed to write settings.schema.json: {e}"))
        })?;
        files_created.push("settings.schema.json".into());
    }

    // 6. Pre-activation validation
    // Verify that the generated manifest parses cleanly as an ExtensionManifest
    let manifest_bytes = fs::read(staging_dir.join("extension.toml")).map_err(|e| {
        ScaffoldError::StagingFailed(format!("failed to read generated manifest: {e}"))
    })?;
    let parsed_manifest = toml::from_slice::<ExtensionManifest>(&manifest_bytes).map_err(|e| {
        ScaffoldError::StagingFailed(format!("generated manifest failed schema validation: {e}"))
    })?;
    assert_eq!(parsed_manifest.id, extension_id);

    // Verify shilpo-ext.json
    let config_bytes = fs::read(staging_dir.join("shilpo-ext.json")).map_err(|e| {
        ScaffoldError::StagingFailed(format!("failed to read generated config: {e}"))
    })?;
    let _parsed_config =
        serde_json::from_slice::<ExtensionProjectConfig>(&config_bytes).map_err(|e| {
            ScaffoldError::StagingFailed(format!(
                "generated shilpo-ext.json failed validation: {e}"
            ))
        })?;

    // 7. Atomic Activation (Rename)
    // A signal may arrive while the generated files are being validated. Keep
    // the staging tree intact for the guard to remove rather than activating
    // a project after the user has cancelled.
    if cancellation_requested() {
        return Err(ScaffoldError::Cancelled);
    }
    if options.target_dir.exists() && options.target_dir.is_dir() {
        // Safe to remove the empty target directory before rename
        let _ = fs::remove_dir(&options.target_dir);
    }

    fs::rename(&staging_dir, &options.target_dir).map_err(|e| {
        ScaffoldError::ActivationFailed(format!(
            "failed to move staging directory '{}' to target '{}': {e}",
            staging_dir.display(),
            options.target_dir.display()
        ))
    })?;

    // Project is now active: defuse the staging cleanup guard
    guard.defuse();

    let display_target = options.target_dir.display().to_string();

    let mut git_initialized = false;
    let mut installed = false;
    let mut built = false;

    // 8. Post-activation Stage: Git Init
    if options.git {
        if cancellation_requested() {
            return Err(ScaffoldError::Cancelled);
        }
        let git_cmd = ProcessCommand::new("git")
            .arg("init")
            .cwd(&options.target_dir);
        match runner.run_with_timeout(&git_cmd, std::time::Duration::from_secs(24 * 60 * 60)) {
            Ok(output) if output.success => {
                git_initialized = true;
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ScaffoldError::StageFailed {
                    stage: "git_init".into(),
                    recovery_command: format!("cd {display_target} && git init"),
                    message: format!(
                        "'git init' exited with code {:?}: {stderr}",
                        output.exit_code
                    ),
                });
            }
            Err(e) if e.contains("cancelled") => return Err(ScaffoldError::Cancelled),
            Err(e) => {
                return Err(ScaffoldError::StageFailed {
                    stage: "git_init".into(),
                    recovery_command: format!("cd {display_target} && git init"),
                    message: e,
                });
            }
        }
    }

    // 9. Post-activation Stage: Install
    let should_install = options.install || options.build;
    if should_install && options.language == StarterLanguage::Typescript {
        if cancellation_requested() {
            return Err(ScaffoldError::Cancelled);
        }
        let pm = options.package_manager.unwrap_or(PackageManager::Npm);
        let install_cmd = ProcessCommand::new(pm.as_str())
            .arg("install")
            .cwd(&options.target_dir);
        match runner.run_with_timeout(&install_cmd, std::time::Duration::from_secs(24 * 60 * 60)) {
            Ok(output) if output.success => {
                installed = true;
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ScaffoldError::StageFailed {
                    stage: "install".into(),
                    recovery_command: format!("cd {display_target} && {} install", pm.as_str()),
                    message: format!(
                        "'{} install' exited with code {:?}: {stderr}",
                        pm.as_str(),
                        output.exit_code
                    ),
                });
            }
            Err(e) if e.contains("cancelled") => return Err(ScaffoldError::Cancelled),
            Err(e) => {
                return Err(ScaffoldError::StageFailed {
                    stage: "install".into(),
                    recovery_command: format!("cd {display_target} && {} install", pm.as_str()),
                    message: e,
                });
            }
        }
    }

    // 10. Post-activation Stage: Build
    if options.build {
        if cancellation_requested() {
            return Err(ScaffoldError::Cancelled);
        }
        let build_res = build_extension(&options.target_dir, false, runner);
        if build_res.success {
            built = true;
        } else {
            let err_msg = build_res
                .diagnostics
                .first()
                .cloned()
                .unwrap_or_else(|| "build failed".into());
            if build_res
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("cancelled"))
            {
                return Err(ScaffoldError::Cancelled);
            }
            return Err(ScaffoldError::StageFailed {
                stage: "build".into(),
                recovery_command: format!("cd {display_target} && shilpo ext build"),
                message: err_msg,
            });
        }
    }

    // 11. Compute next steps
    let mut next_steps = vec![format!("cd {display_target}")];
    if options.language == StarterLanguage::Typescript && !installed && !built {
        let pm = options.package_manager.unwrap_or(PackageManager::Npm);
        next_steps.push(format!("{} install", pm.as_str()));
    }
    if !built {
        next_steps.push("shilpo ext build".into());
    }
    next_steps.push("shilpo ext dev".into());

    Ok(ScaffoldResult {
        name: options.name.clone(),
        extension_id,
        language: options.language,
        contribution: options.contribution,
        target_dir: options.target_dir.clone(),
        files_created,
        installed,
        built,
        git_initialized,
        next_steps,
    })
}

fn cancellation_requested() -> bool {
    PROCESS_CANCELLED.load(std::sync::atomic::Ordering::Acquire)
}

extern "C" fn scaffold_signal_handler(_signal: libc::c_int) {
    PROCESS_CANCELLED.store(true, std::sync::atomic::Ordering::Release);
}

fn install_cancel_handlers() {
    // SAFETY: installing a process-local flag-only handler is async-signal-safe.
    unsafe {
        libc::signal(
            libc::SIGINT,
            scaffold_signal_handler as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            scaffold_signal_handler as *const () as libc::sighandler_t,
        );
    }
}

// ---------------------------------------------------------------------------
// Template Generators
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn generate_manifest(
    name: &str,
    extension_id: &ExtensionId,
    package_name: &str,
    description: Option<&str>,
    language: StarterLanguage,
    contribution: StarterContribution,
    capabilities: &[Capability],
    subscriptions: &[Subscription],
) -> Result<String, ScaffoldError> {
    let mut manifest = ExtensionManifest {
        id: extension_id.clone(),
        name: name.to_string(),
        version: Version::parse("0.1.0").unwrap(),
        schema_version: SUPPORTED_SCHEMA_VERSION,
        api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
        min_shilpo_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
        authors: Vec::new(),
        description: description.map(|d| d.to_string()),
        repository: None,
        license: None,
        library: Some(LibraryConfig {
            path: match language {
                StarterLanguage::Rust => {
                    format!("target/wasm32-wasip2/release/{package_name}.wasm")
                }
                StarterLanguage::Typescript => format!("dist/{package_name}.wasm"),
            },
        }),
        contributions: Contributions::default(),
        subscriptions: subscriptions.to_vec(),
        capabilities: capabilities.to_vec(),
    };

    match contribution {
        StarterContribution::BarWidget => {
            manifest
                .contributions
                .bar_widgets
                .push(BarWidgetContribution {
                    id: ContributionId::new("widget").unwrap(),
                    name: name.to_string(),
                    description: None,
                });
        }
        StarterContribution::DesktopWidget => {
            manifest
                .contributions
                .desktop_widgets
                .push(DesktopWidgetContribution {
                    id: ContributionId::new("widget").unwrap(),
                    name: name.to_string(),
                    description: None,
                    default_width: None,
                    default_height: None,
                    min_width: None,
                    min_height: None,
                });
        }
        StarterContribution::SettingsPage => {
            manifest
                .contributions
                .settings_pages
                .push(SettingsPageContribution {
                    id: ContributionId::new("settings").unwrap(),
                    name: format!("{name} Settings"),
                    schema: "settings.schema.json".into(),
                });
        }
        StarterContribution::SidePanel => {
            manifest
                .contributions
                .side_panels
                .push(SidePanelContribution {
                    id: ContributionId::new("panel").unwrap(),
                    name: format!("{name} Panel"),
                });
        }
        StarterContribution::Action => {
            manifest.contributions.actions.push(ActionContribution {
                id: ContributionId::new("run").unwrap(),
                name: format!("{name} Action"),
            });
        }
        StarterContribution::Empty => {}
    }

    let toml_body = toml::to_string_pretty(&manifest)
        .map_err(|e| ScaffoldError::StagingFailed(format!("failed to serialize manifest: {e}")))?;

    Ok(format!("# Generated by Shilpo CLI v0.1.0\n{toml_body}"))
}

fn generate_project_config(language: StarterLanguage) -> String {
    let config = match language {
        StarterLanguage::Rust => ExtensionProjectConfig {
            language: ExtensionLanguage::Rust,
            entry: None,
            crate_dir: Some(".".into()),
        },
        StarterLanguage::Typescript => ExtensionProjectConfig {
            language: ExtensionLanguage::Typescript,
            entry: Some("src/extension.ts".into()),
            crate_dir: None,
        },
    };

    serde_json::to_string_pretty(&config).unwrap_or_else(|_| "{}".into()) + "\n"
}

fn generate_cargo_toml(package_name: &str, description: Option<&str>) -> String {
    let desc_line = description
        .map(|d| format!("description = {:?}\n", d))
        .unwrap_or_default();

    format!(
        r#"# Generated by Shilpo CLI v0.1.0
[package]
name = "{package_name}"
version = "0.1.0"
edition = "2024"
{desc_line}
[lib]
crate-type = ["cdylib"]

[dependencies]
shilpo-ext-sdk = {{ git = "https://github.com/sayeed205/shilpo.git" }}
"#
    )
}

fn generate_rust_source(name: &str, contribution: StarterContribution) -> String {
    let safe_name = escape_rust_string_contents(name);
    match contribution {
        StarterContribution::BarWidget => {
            format!(
                r#"// Generated by Shilpo CLI v0.1.0
use shilpo_ext_sdk::prelude::*;

#[derive(Default)]
struct ExtensionState {{
    clicks: i64,
}}

impl Extension for ExtensionState {{
    fn activate(&mut self, _activation: Activation) -> Result<(), Error> {{
        Ok(())
    }}

    fn on_event(&mut self, event: ExtensionEvent) -> Result<(), Error> {{
        if let ExtensionEvent::Input(input) = event
            && input.event_id == "increment"
        {{
            self.clicks += 1;
        }}
        Ok(())
    }}

    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {{
        if contribution_id != "widget" {{
            return Ok(None);
        }}

        Ok(Some(view! {{
            row {{
                icon("star").size(16.0),
                    text(format!("{safe_name}: {{}}", self.clicks)).bold(true),
                button("+1", "increment"),
            }}
        }}))
    }}
}}

export_extension!(ExtensionState);
"#
            )
        }
        StarterContribution::DesktopWidget => {
            format!(
                r#"// Generated by Shilpo CLI v0.1.0
use shilpo_ext_sdk::prelude::*;

#[derive(Default)]
struct ExtensionState;

impl Extension for ExtensionState {{
    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {{
        if contribution_id != "widget" {{
            return Ok(None);
        }}

        Ok(Some(view! {{
            column {{
                row {{
                    icon("dashboard").size(20.0),
                    text("{safe_name}").bold(true).font_size(16.0),
                }},
                divider(),
                text("Desktop widget content"),
            }}
        }}))
    }}
}}

export_extension!(ExtensionState);
"#
            )
        }
        StarterContribution::SettingsPage => {
            format!(
                r#"// Generated by Shilpo CLI v0.1.0
use shilpo_ext_sdk::prelude::*;

#[derive(Default)]
struct ExtensionState {{
    enabled: bool,
}}

impl Extension for ExtensionState {{
    fn activate(&mut self, _activation: Activation) -> Result<(), Error> {{
        self.enabled = true;
        Ok(())
    }}

    fn on_event(&mut self, event: ExtensionEvent) -> Result<(), Error> {{
        if let ExtensionEvent::Input(input) = event
            && input.event_id == "toggle-enabled"
        {{
            self.enabled = !self.enabled;
        }}
        Ok(())
    }}

    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {{
        if contribution_id != "settings" {{
            return Ok(None);
        }}

        Ok(Some(view! {{
            column {{
        text("{safe_name} Settings").bold(true).font_size(18.0),
                divider(),
                row {{
                    text("Enable Feature"),
                    spacer(),
                    toggle(self.enabled, "toggle-enabled"),
                }},
            }}
        }}))
    }}
}}

export_extension!(ExtensionState);
"#
            )
        }
        StarterContribution::SidePanel => {
            format!(
                r#"// Generated by Shilpo CLI v0.1.0
use shilpo_ext_sdk::prelude::*;

#[derive(Default)]
struct ExtensionState;

impl Extension for ExtensionState {{
    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {{
        if contribution_id != "panel" {{
            return Ok(None);
        }}

        Ok(Some(view! {{
            column {{
                row {{
                    icon("sidebar").size(18.0),
        text("{safe_name} Panel").bold(true),
                }},
                divider(),
                text("Side panel content"),
            }}
        }}))
    }}
}}

export_extension!(ExtensionState);
"#
            )
        }
        StarterContribution::Action => r#"// Generated by Shilpo CLI v0.1.0
use shilpo_ext_sdk::prelude::*;

#[derive(Default)]
struct ExtensionState;

impl Extension for ExtensionState {
    fn activate(&mut self, _activation: Activation) -> Result<(), Error> {
        Ok(())
    }

    fn on_event(&mut self, _event: ExtensionEvent) -> Result<(), Error> {
        Ok(())
    }
}

export_extension!(ExtensionState);
"#
        .to_string(),
        StarterContribution::Empty => r#"// Generated by Shilpo CLI v0.1.0
use shilpo_ext_sdk::prelude::*;

#[derive(Default)]
struct ExtensionState;

impl Extension for ExtensionState {}

export_extension!(ExtensionState);
"#
        .to_string(),
    }
}

fn generate_rust_readme(name: &str, description: Option<&str>) -> String {
    let desc = description.unwrap_or("A Shilpo desktop extension written in Rust.");
    format!(
        r#"# {name}

{desc}

## Development

### Lint
```bash
shilpo ext lint
```

### Build
```bash
shilpo ext build
```

### Hot-Reload Development
```bash
shilpo ext dev
```

See [Shilpo Extension Documentation](https://github.com/sayeed205/shilpo/blob/main/docs/extensions/index.md) for authoring guides, API references, and testing practices.

> **Note**: `shilpo-ext-sdk` currently references the GitHub repository while the crate is prepared for release.
"#
    )
}

fn generate_package_json(package_name: &str, description: Option<&str>) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "name": package_name,
        "version": "0.1.0",
        "type": "module",
        "description": description.unwrap_or(""),
        "scripts": {"build": "shilpo ext build"},
        "dependencies": {"@shilpo/ext-sdk": "npm:@jsr/shilpo__ext-sdk@^0.1.0"},
        "devDependencies": {"@bytecodealliance/jco": "1.28.1"}
    }))
    .expect("package metadata is serializable")
        + "\n"
}

fn generate_tsconfig_json() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["src/**/*"]
}
"#
    .to_string()
}

fn generate_typescript_source(name: &str, contribution: StarterContribution) -> String {
    let safe_name = escape_js_string_contents(name);
    let safe_template_name = escape_js_template_contents(name);
    match contribution {
        StarterContribution::BarWidget => {
            format!(
                r#"// Generated by Shilpo CLI v0.1.0
import {{ defineExtension, row, icon, text, button, Alignment }} from "@shilpo/ext-sdk";

let clicks = 0;

const ext = defineExtension({{
  onActivate(_activation, _host) {{
    clicks = 0;
  }},

  onInput(event, _host) {{
    if (event.eventId === "increment") {{
      clicks += 1;
    }}
  }},

  view(contributionId) {{
    if (contributionId !== "widget") {{
      return undefined;
    }}

    return row({{
      gap: 8,
      alignItems: "center" as Alignment,
      children: [
        icon("star", {{ size: 16 }}),
        text(`{safe_template_name}: ${{clicks}}`, {{ bold: true }}),
        button("+1", "increment"),
      ],
    }});
  }},
}});

export const activate = ext.activate;
export const deactivate = ext.deactivate;
export const onEvent = ext.onEvent;
export const view = ext.view;
"#
            )
        }
        StarterContribution::DesktopWidget => {
            format!(
                r#"// Generated by Shilpo CLI v0.1.0
import {{ defineExtension, column, row, icon, text, divider, Alignment }} from "@shilpo/ext-sdk";

const ext = defineExtension({{
  view(contributionId) {{
    if (contributionId !== "widget") {{
      return undefined;
    }}

    return column({{
      gap: 12,
      children: [
        row({{
          gap: 8,
          alignItems: "center" as Alignment,
          children: [
            icon("dashboard", {{ size: 20 }}),
            text("{safe_name}", {{ bold: true, fontSize: 16 }}),
          ],
        }}),
        divider(),
        text("Desktop widget content"),
      ],
    }});
  }},
}});

export const activate = ext.activate;
export const deactivate = ext.deactivate;
export const onEvent = ext.onEvent;
export const view = ext.view;
"#
            )
        }
        StarterContribution::SettingsPage => {
            format!(
                r#"// Generated by Shilpo CLI v0.1.0
import {{ defineExtension, column, row, text, divider, spacer, toggle, Alignment }} from "@shilpo/ext-sdk";

let enabled = true;

const ext = defineExtension({{
  onActivate(_activation, _host) {{
    enabled = true;
  }},

  onInput(event, _host) {{
    if (event.eventId === "toggle-enabled") {{
      enabled = !enabled;
    }}
  }},

  view(contributionId) {{
    if (contributionId !== "settings") {{
      return undefined;
    }}

    return column({{
      gap: 12,
      children: [
        text("{safe_name} Settings", {{ bold: true, fontSize: 18 }}),
        divider(),
        row({{
          gap: 8,
          alignItems: "center" as Alignment,
          children: [
            text("Enable Feature"),
            spacer(),
            toggle(enabled, "toggle-enabled"),
          ],
        }}),
      ],
    }});
  }},
}});

export const activate = ext.activate;
export const deactivate = ext.deactivate;
export const onEvent = ext.onEvent;
export const view = ext.view;
"#
            )
        }
        StarterContribution::SidePanel => {
            format!(
                r#"// Generated by Shilpo CLI v0.1.0
import {{ defineExtension, column, row, icon, text, divider, Alignment }} from "@shilpo/ext-sdk";

const ext = defineExtension({{
  view(contributionId) {{
    if (contributionId !== "panel") {{
      return undefined;
    }}

    return column({{
      gap: 8,
      children: [
        row({{
          gap: 8,
          alignItems: "center" as Alignment,
          children: [
            icon("sidebar", {{ size: 18 }}),
        text("{safe_name} Panel", {{ bold: true }}),
          ],
        }}),
        divider(),
        text("Side panel content"),
      ],
    }});
  }},
}});

export const activate = ext.activate;
export const deactivate = ext.deactivate;
export const onEvent = ext.onEvent;
export const view = ext.view;
"#
            )
        }
        StarterContribution::Action => r#"// Generated by Shilpo CLI v0.1.0
import { defineExtension } from "@shilpo/ext-sdk";

const ext = defineExtension({
  onActivate(_activation, _host) {},
  onEvent(_event, _host) {},
});

export const activate = ext.activate;
export const deactivate = ext.deactivate;
export const onEvent = ext.onEvent;
export const view = ext.view;
"#
        .to_string(),
        StarterContribution::Empty => r#"// Generated by Shilpo CLI v0.1.0
import { defineExtension } from "@shilpo/ext-sdk";

const ext = defineExtension({});

export const activate = ext.activate;
export const deactivate = ext.deactivate;
export const onEvent = ext.onEvent;
export const view = ext.view;
"#
        .to_string(),
    }
}

fn generate_typescript_readme(
    name: &str,
    description: Option<&str>,
    package_manager: PackageManager,
) -> String {
    let desc = description.unwrap_or("A Shilpo desktop extension written in TypeScript.");
    let pm = package_manager.as_str();
    format!(
        r#"# {name}

{desc}

## Development

### Install Dependencies
```bash
{pm} install
```

### Lint
```bash
shilpo ext lint
```

### Build
```bash
shilpo ext build
```

### Hot-Reload Development
```bash
shilpo ext dev
```

See [Shilpo Extension Documentation](https://github.com/sayeed205/shilpo/blob/main/docs/extensions/index.md) for authoring guides, API references, and testing practices.
"#
    )
}

fn generate_settings_schema(name: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": format!("{name} Settings"),
        "type": "object",
        "properties": {"enabled": {
            "type": "boolean",
            "default": true,
            "description": "Enable or disable extension features"
        }},
        "additionalProperties": false
    }))
    .expect("settings schema is serializable")
        + "\n"
}

fn escape_rust_string_contents(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn escape_js_string_contents(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn escape_js_template_contents(value: &str) -> String {
    escape_js_string_contents(value)
        .replace('`', "\\`")
        .replace("${", "\\${")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::ProcessOutput;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tempfile::tempdir;

    struct MockProcessRunner {
        commands: Mutex<Vec<ProcessCommand>>,
        responses: Mutex<HashMap<String, Result<ProcessOutput, String>>>,
    }

    impl MockProcessRunner {
        fn new() -> Self {
            Self {
                commands: Mutex::new(Vec::new()),
                responses: Mutex::new(HashMap::new()),
            }
        }

        fn with_response(self, program: &str, output: Result<ProcessOutput, String>) -> Self {
            self.responses
                .lock()
                .unwrap()
                .insert(program.into(), output);
            self
        }

        fn recorded_commands(&self) -> Vec<ProcessCommand> {
            self.commands.lock().unwrap().clone()
        }
    }

    impl ProcessRunner for MockProcessRunner {
        fn which(&self, binary_name: &str) -> Option<PathBuf> {
            Some(PathBuf::from(format!("/mock/bin/{binary_name}")))
        }

        fn run(&self, command: &ProcessCommand) -> Result<ProcessOutput, String> {
            self.commands.lock().unwrap().push(command.clone());
            if let Some(resp) = self.responses.lock().unwrap().get(&command.program) {
                return resp.clone();
            }
            Ok(ProcessOutput {
                success: true,
                exit_code: Some(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn test_derive_extension_id() {
        assert_eq!(
            derive_extension_id("My Extension", None).unwrap().as_str(),
            "dev.local.my-extension"
        );
        assert_eq!(
            derive_extension_id("weather", None).unwrap().as_str(),
            "dev.local.weather"
        );
        assert_eq!(
            derive_extension_id("io.github.alice.notes", None)
                .unwrap()
                .as_str(),
            "io.github.alice.notes"
        );
        assert_eq!(
            derive_extension_id("Custom", Some("org.example.custom"))
                .unwrap()
                .as_str(),
            "org.example.custom"
        );
        assert!(derive_extension_id("", None).is_err());
        assert!(derive_extension_id("---", None).is_err());
        assert!(derive_extension_id("My Ext", Some("invalid_id")).is_err());
    }

    #[test]
    fn test_derive_package_name_rust() {
        assert_eq!(
            derive_package_name("My Extension", StarterLanguage::Rust, None).unwrap(),
            "my_extension"
        );
        assert_eq!(
            derive_package_name("123-weather", StarterLanguage::Rust, None).unwrap(),
            "ext_123_weather"
        );
        assert_eq!(
            derive_package_name("Foo", StarterLanguage::Rust, Some("custom_crate")).unwrap(),
            "custom_crate"
        );
        assert!(derive_package_name("Foo", StarterLanguage::Rust, Some("123bad")).is_err());
    }

    #[test]
    fn test_derive_package_name_typescript() {
        assert_eq!(
            derive_package_name("My Extension", StarterLanguage::Typescript, None).unwrap(),
            "my-extension"
        );
        assert_eq!(
            derive_package_name("Foo", StarterLanguage::Typescript, Some("my-cool-pkg")).unwrap(),
            "my-cool-pkg"
        );
    }

    #[test]
    fn generated_metadata_uses_exact_jco_and_escapes_json() {
        let package = generate_package_json("demo", Some("quoted \"description\"\nline"));
        let value: serde_json::Value = serde_json::from_str(&package).unwrap();
        assert_eq!(value["devDependencies"]["@bytecodealliance/jco"], "1.28.1");
        assert_eq!(value["description"], "quoted \"description\"\nline");
    }

    #[test]
    fn capability_sync_rejects_conflicts_and_wildcards() {
        let explicit = [Capability::EventsSubscribe {
            events: vec![EventKind::ThemeChanged],
        }];
        let subscriptions = [Subscription {
            event: EventKind::OutputsChanged,
        }];
        assert!(matches!(
            synchronize_capabilities_and_subscriptions(&explicit, &subscriptions),
            Err(ScaffoldError::CapabilityConflict(_))
        ));

        let wildcard = [Capability::FilesystemRead {
            paths: vec!["/var/*".into()],
        }];
        assert!(matches!(
            synchronize_capabilities_and_subscriptions(&wildcard, &[]),
            Err(ScaffoldError::InvalidCapability(_))
        ));
    }

    #[test]
    fn generated_sources_escape_display_values() {
        let name = "A \"quoted\"\nname";
        let rust = generate_rust_source(name, StarterContribution::BarWidget);
        let typescript = generate_typescript_source(name, StarterContribution::BarWidget);
        assert!(!rust.contains("A \"quoted\"\nname"));
        assert!(!typescript.contains("A \"quoted\"\nname"));
        assert!(rust.contains("A \\\"quoted\\\"\\nname"));
        assert!(typescript.contains("A \\\"quoted\\\"\\nname"));
        let schema: serde_json::Value =
            serde_json::from_str(&generate_settings_schema(name)).unwrap();
        assert!(schema["title"].as_str().unwrap().starts_with(name));
    }

    #[test]
    fn test_validate_target_path() {
        let temp = tempdir().unwrap();
        let valid_new = temp.path().join("new_dir");
        assert!(validate_target_path(&valid_new).is_ok());

        let empty_existing = temp.path().join("empty_dir");
        fs::create_dir_all(&empty_existing).unwrap();
        assert!(validate_target_path(&empty_existing).is_ok());

        let non_empty = temp.path().join("non_empty");
        fs::create_dir_all(&non_empty).unwrap();
        fs::write(non_empty.join("file.txt"), "data").unwrap();
        assert!(matches!(
            validate_target_path(&non_empty),
            Err(ScaffoldError::TargetExistsAndNotEmpty(_))
        ));

        let a_file = temp.path().join("file.txt");
        fs::write(&a_file, "data").unwrap();
        assert!(matches!(
            validate_target_path(&a_file),
            Err(ScaffoldError::TargetIsFile(_))
        ));

        let traversal = PathBuf::from("a/../b");
        assert!(matches!(
            validate_target_path(&traversal),
            Err(ScaffoldError::PathTraversal(_))
        ));
    }

    #[test]
    fn test_capability_subscription_synchronization() {
        let caps = vec![Capability::NetworkHttp {
            hosts: vec!["api.weather.com".into()],
            paths: vec!["/v1/weather".into()],
        }];
        let subs = vec![
            Subscription {
                event: EventKind::OutputsChanged,
            },
            Subscription {
                event: EventKind::OutputsChanged, // duplicate
            },
            Subscription {
                event: EventKind::ThemeChanged,
            },
        ];

        let (merged_caps, merged_subs) =
            synchronize_capabilities_and_subscriptions(&caps, &subs).unwrap();
        assert_eq!(merged_subs.len(), 2);
        assert!(merged_caps.iter().any(|c| matches!(
            c,
            Capability::EventsSubscribe { events } if events.contains(&EventKind::OutputsChanged) && events.contains(&EventKind::ThemeChanged)
        )));
    }

    #[test]
    fn test_scaffold_all_12_combinations() {
        let languages = [StarterLanguage::Rust, StarterLanguage::Typescript];
        let contributions = [
            StarterContribution::BarWidget,
            StarterContribution::DesktopWidget,
            StarterContribution::SettingsPage,
            StarterContribution::SidePanel,
            StarterContribution::Action,
            StarterContribution::Empty,
        ];

        for lang in languages {
            for contrib in contributions {
                let temp = tempdir().unwrap();
                let target = temp.path().join(format!("ext-{lang}-{contrib}"));
                let options = ScaffoldOptions {
                    name: format!("Test {lang} {contrib}"),
                    target_dir: target.clone(),
                    language: lang,
                    contribution: contrib,
                    package_manager: if lang == StarterLanguage::Typescript {
                        Some(PackageManager::Npm)
                    } else {
                        None
                    },
                    extension_id: None,
                    package_name: None,
                    description: Some("Test description".into()),
                    capabilities: Vec::new(),
                    subscriptions: Vec::new(),
                    install: false,
                    build: false,
                    git: false,
                };

                let runner = MockProcessRunner::new();
                let res = scaffold_extension(&options, &runner).unwrap();
                assert_eq!(res.language, lang);
                assert_eq!(res.contribution, contrib);
                assert!(target.exists());

                // Check extension.toml
                let manifest_str = fs::read_to_string(target.join("extension.toml")).unwrap();
                let manifest: ExtensionManifest = toml::from_str(&manifest_str).unwrap();
                assert_eq!(manifest.name, format!("Test {lang} {contrib}"));
                assert!(!manifest_str.contains("<name>"));
                assert!(!manifest_str.contains("{name}"));

                // Check shilpo-ext.json
                let config_str = fs::read_to_string(target.join("shilpo-ext.json")).unwrap();
                let config: ExtensionProjectConfig = serde_json::from_str(&config_str).unwrap();
                match lang {
                    StarterLanguage::Rust => {
                        assert_eq!(config.language, ExtensionLanguage::Rust);
                        assert!(target.join("Cargo.toml").exists());
                        assert!(target.join("src/lib.rs").exists());
                        let lib_content = fs::read_to_string(target.join("src/lib.rs")).unwrap();
                        assert!(!lib_content.contains("<name>"));
                        assert!(!lib_content.contains("{name}"));
                    }
                    StarterLanguage::Typescript => {
                        assert_eq!(config.language, ExtensionLanguage::Typescript);
                        assert!(target.join("package.json").exists());
                        assert!(target.join("tsconfig.json").exists());
                        assert!(target.join(".npmrc").exists());
                        assert!(target.join("src/extension.ts").exists());
                        let ts_content =
                            fs::read_to_string(target.join("src/extension.ts")).unwrap();
                        assert!(!ts_content.contains("<name>"));
                        assert!(!ts_content.contains("{name}"));
                    }
                }

                // Check settings schema if SettingsPage
                if contrib == StarterContribution::SettingsPage {
                    assert!(target.join("settings.schema.json").exists());
                    let schema_str =
                        fs::read_to_string(target.join("settings.schema.json")).unwrap();
                    let schema_val: serde_json::Value = serde_json::from_str(&schema_str).unwrap();
                    assert!(schema_val.is_object());
                }

                // Check README.md
                assert!(target.join("README.md").exists());
                let readme = fs::read_to_string(target.join("README.md")).unwrap();
                assert!(!readme.contains("<name>"));
                assert!(!readme.contains("{name}"));
                assert!(!readme.contains("rtk"));
            }
        }
    }

    #[test]
    fn test_scaffold_with_git_and_install_mocked() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("ts-full");
        let options = ScaffoldOptions {
            name: "TS Full".into(),
            target_dir: target.clone(),
            language: StarterLanguage::Typescript,
            contribution: StarterContribution::BarWidget,
            package_manager: Some(PackageManager::Pnpm),
            extension_id: None,
            package_name: None,
            description: None,
            capabilities: Vec::new(),
            subscriptions: Vec::new(),
            install: true,
            build: false,
            git: true,
        };

        let runner = MockProcessRunner::new();
        let res = scaffold_extension(&options, &runner).unwrap();
        assert!(res.git_initialized);
        assert!(res.installed);

        let cmds = runner.recorded_commands();
        assert!(
            cmds.iter()
                .any(|c| c.program == "git" && c.args == vec!["init"])
        );
        assert!(
            cmds.iter()
                .any(|c| c.program == "pnpm" && c.args == vec!["install"])
        );
    }

    #[test]
    fn test_git_failure_preserves_target() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("git-fail");
        let options = ScaffoldOptions {
            name: "Git Fail".into(),
            target_dir: target.clone(),
            language: StarterLanguage::Rust,
            contribution: StarterContribution::Empty,
            package_manager: None,
            extension_id: None,
            package_name: None,
            description: None,
            capabilities: Vec::new(),
            subscriptions: Vec::new(),
            install: false,
            build: false,
            git: true,
        };

        let runner =
            MockProcessRunner::new().with_response("git", Err("git command not found".into()));

        let err = scaffold_extension(&options, &runner).unwrap_err();
        assert!(matches!(err, ScaffoldError::StageFailed { ref stage, .. } if stage == "git_init"));
        assert!(
            target.exists(),
            "Target directory must be preserved on post-activation failure"
        );
        assert!(target.join("extension.toml").exists());
    }

    #[test]
    fn test_install_failure_preserves_target() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("install-fail");
        let options = ScaffoldOptions {
            name: "Install Fail".into(),
            target_dir: target.clone(),
            language: StarterLanguage::Typescript,
            contribution: StarterContribution::Empty,
            package_manager: Some(PackageManager::Bun),
            extension_id: None,
            package_name: None,
            description: None,
            capabilities: Vec::new(),
            subscriptions: Vec::new(),
            install: true,
            build: false,
            git: false,
        };

        let runner = MockProcessRunner::new().with_response(
            "bun",
            Ok(ProcessOutput {
                success: false,
                exit_code: Some(1),
                stdout: Vec::new(),
                stderr: b"network failure".to_vec(),
            }),
        );

        let err = scaffold_extension(&options, &runner).unwrap_err();
        assert!(matches!(err, ScaffoldError::StageFailed { ref stage, .. } if stage == "install"));
        assert!(
            target.exists(),
            "Target directory must be preserved on post-activation failure"
        );
    }

    #[test]
    fn test_rust_rejects_package_manager() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("rust-pm");
        let options = ScaffoldOptions {
            name: "Rust PM".into(),
            target_dir: target,
            language: StarterLanguage::Rust,
            contribution: StarterContribution::Empty,
            package_manager: Some(PackageManager::Npm),
            extension_id: None,
            package_name: None,
            description: None,
            capabilities: Vec::new(),
            subscriptions: Vec::new(),
            install: false,
            build: false,
            git: false,
        };

        let runner = MockProcessRunner::new();
        let err = scaffold_extension(&options, &runner).unwrap_err();
        assert!(matches!(err, ScaffoldError::InvalidTarget(_)));
    }
}
