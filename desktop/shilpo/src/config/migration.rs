//! Ordered, primary-only configuration schema migrations.
//!
//! The canonical hand-authored `config.toml` is the only migration-owned
//! source (ADR-0010). Migrations operate on [`toml_edit::DocumentMut`] so
//! comments, whitespace, ordering, quoted keys, arrays, and inline tables
//! survive. A migration is committed only after the migrated primary
//! document and the complete layered candidate validate successfully, an
//! exact byte-for-byte backup is durable, and an atomic same-directory
//! replacement succeeds.
//!
//! Version contract:
//! - A missing top-level `version` in a non-empty primary is schema 0.
//! - An explicit integer `version = 0` is also schema 0.
//! - `version = 1` is current; anything newer is a hard
//!   [`MigrationError::FutureVersion`] error that is never downgraded.
//! - Negative, noninteger, or out-of-range values are invalid version
//!   diagnostics. `version` is metadata: it never participates in the
//!   resolver's [`crate::config::RecoveryScope`] machinery.

use std::{
    fmt, fs, io,
    io::Write,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use serde::Serialize;
use toml_edit::{DocumentMut, Item, Value};

use crate::config::{
    merge::read_source_document,
    resolver::ConfigResolver,
    source::{ConfigSource, discover_sources},
    types::{ConfigDiagnostic, ConfigError},
    unknown_keys::UnknownConfigKey,
};

/// The single authoritative latest declarative config schema version.
///
/// Used by [`crate::config::ShellConfig::default`], validation, migration
/// planning, diagnostics, and tests. Never repeat a literal version across
/// modules.
pub const LATEST_CONFIG_VERSION: u32 = 1;

/// A single ordered migration step that mutates an in-memory TOML document
/// and reports failure with a human message.
#[derive(Clone, Debug)]
pub struct Migration {
    /// Schema version this step migrates from.
    pub from: u32,
    /// Schema version this step migrates to (`to == from + 1`).
    pub to: u32,
    /// Stable step name surfaced in plans, outcomes, and diagnostics.
    pub name: &'static str,
    apply: fn(&mut DocumentMut) -> Result<(), String>,
}

impl Migration {
    pub fn new(
        from: u32,
        to: u32,
        name: &'static str,
        apply: fn(&mut DocumentMut) -> Result<(), String>,
    ) -> Self {
        Self {
            from,
            to,
            name,
            apply,
        }
    }
}

/// Ordered, immutable set of migration steps terminating at a latest version.
///
/// Construction validates the registry invariants:
/// - exactly one step begins at each supported old version,
/// - every step advances by exactly one version (`to == from + 1`),
/// - steps are contiguous (no gaps) and terminate at the registry's latest
///   version,
/// - no duplicate `from` versions.
#[derive(Clone, Debug)]
pub struct MigrationRegistry {
    steps: Vec<Migration>,
    latest: u32,
}

impl MigrationRegistry {
    /// The production registry: the real v0 -> v1 boundary plus all future
    /// steps. Panics are impossible by construction (invariants hold).
    pub fn production() -> Self {
        Self::new(
            vec![Migration::new(0, 1, "v0 -> v1", v0_to_v1)],
            LATEST_CONFIG_VERSION,
        )
        .expect("production migration registry satisfies its invariants")
    }

    /// Validate the registry invariants before it may plan or execute.
    pub fn new(steps: Vec<Migration>, latest: u32) -> Result<Self, MigrationError> {
        let mut steps = steps;
        steps.sort_by_key(|step| step.from);

        for pair in steps.windows(2) {
            if pair[0].from == pair[1].from {
                return Err(MigrationError::RegistryDuplicate { from: pair[0].from });
            }
        }
        for step in &steps {
            if step.to != step.from + 1 {
                return Err(MigrationError::RegistryNonUnitJump {
                    from: step.from,
                    to: step.to,
                });
            }
        }
        for pair in steps.windows(2) {
            if pair[0].to != pair[1].from {
                return Err(MigrationError::RegistryGap {
                    from: pair[0].to,
                    to: pair[1].from,
                });
            }
        }
        match steps.last() {
            Some(last) if last.to == latest => {}
            Some(last) if last.to > latest => {
                return Err(MigrationError::RegistryExceedsLatest {
                    from: last.from,
                    to: last.to,
                    latest,
                });
            }
            _ => return Err(MigrationError::RegistryUnreachable { latest }),
        }
        Ok(Self { steps, latest })
    }

    /// The version this registry migrates toward.
    pub fn latest(&self) -> u32 {
        self.latest
    }

    /// Every registered step, in execution order.
    pub fn steps(&self) -> &[Migration] {
        &self.steps
    }

    /// Build the ordered plan for a document starting at `from_version`.
    ///
    /// A plan with no steps means the document is already current. Future and
    /// unsupported-old versions are hard errors; the registry never downgrades.
    pub fn plan(
        &self,
        from_version: u32,
        path: &Path,
    ) -> Result<MigrationPlan<'_>, MigrationError> {
        if from_version == self.latest {
            return Ok(MigrationPlan {
                from_version,
                to_version: self.latest,
                steps: Vec::new(),
            });
        }
        if from_version > self.latest {
            return Err(MigrationError::FutureVersion {
                path: path.to_path_buf(),
                found: from_version,
                latest: self.latest,
            });
        }
        let Some(oldest) = self.steps.first() else {
            return Err(MigrationError::UnsupportedOldVersion {
                path: path.to_path_buf(),
                found: from_version,
                oldest: self.latest,
            });
        };
        if from_version < oldest.from {
            return Err(MigrationError::UnsupportedOldVersion {
                path: path.to_path_buf(),
                found: from_version,
                oldest: oldest.from,
            });
        }
        let steps = self
            .steps
            .iter()
            .filter(|step| step.from >= from_version)
            .collect();
        Ok(MigrationPlan {
            from_version,
            to_version: self.latest,
            steps,
        })
    }
}

/// Shell startup integration: migrate an existing primary before layered
/// resolution.
///
/// Runs the migration service in [`MigrationMode::Apply`]. A missing or empty
/// primary returns a `Current` outcome without any write, preserving the
/// caller's first-run creation path. When a migration was applied, one
/// structured `tracing::info!` event reports the original/final versions,
/// ordered step names, primary path, and backup path.
///
/// Errors are returned to the caller, which must log them and fall back to
/// its degraded/default policy without rewriting or downgrading the source.
pub fn migrate_primary_for_startup(config_path: &Path) -> Result<MigrationOutcome, MigrationError> {
    let service = MigrationService::for_primary_path(config_path);
    let outcome = service.run(MigrationMode::Apply)?;
    if outcome.changed {
        tracing::info!(
            from_version = outcome.from_version,
            to_version = outcome.to_version,
            steps = %outcome.steps.join(","),
            primary = %outcome.path.display(),
            backup = outcome
                .backup_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            "migrated config.toml to the latest schema before resolution",
        );
    }
    Ok(outcome)
}

/// Every migration step that will run for a given starting version, in order.
#[derive(Clone, Debug)]
pub struct MigrationPlan<'a> {
    /// Schema version of the document before the plan runs.
    pub from_version: u32,
    /// Schema version of the document after the plan runs.
    pub to_version: u32,
    /// Ordered steps, empty when the document is already current.
    pub steps: Vec<&'a Migration>,
}

impl MigrationPlan<'_> {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// Read-only status of the primary document used by manual reload (which
/// never writes or auto-migrates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimaryStatus {
    /// `config.toml` does not exist; nothing to migrate.
    Missing,
    /// `config.toml` exists but contains only whitespace; never rewritten.
    Empty,
    /// Already at the latest version.
    Current,
    /// An older schema version that requires `shilpo config migrate`.
    NeedsMigration { from_version: u32 },
}

/// Why a manual reload is blocked, or `None` when reload may proceed.
///
/// Reload is strictly read-only: the caller keeps the previous committed
/// snapshot and surfaces this message to the user.
pub fn reload_block_reason(status: &PrimaryStatus, path: &Path) -> Option<String> {
    match status {
        PrimaryStatus::NeedsMigration { from_version } => Some(format!(
            "{} requires schema migration from version {from_version}; run 'shilpo config migrate'",
            path.display()
        )),
        PrimaryStatus::Missing | PrimaryStatus::Empty | PrimaryStatus::Current => None,
    }
}

/// The execution mode of a migration run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationMode {
    /// Plan and validate only; zero filesystem writes.
    Preview,
    /// Plan, validate, back up, and atomically replace the primary.
    Apply,
}

/// The result of a migration run, in a flat shape that serializes stably for
/// the `shilpo config migrate` JSON contract.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MigrationOutcome {
    /// Mode this outcome was produced in.
    pub mode: MigrationMode,
    /// Whether a migration was required and (in `Apply`) committed.
    pub changed: bool,
    /// Path of the primary document.
    pub path: PathBuf,
    /// Schema version before migration.
    pub from_version: u32,
    /// Schema version after migration.
    pub to_version: u32,
    /// Ordered step names that ran (or would run).
    pub steps: Vec<String>,
    /// Byte-for-byte backup path, `None` unless a migration was applied.
    pub backup_path: Option<PathBuf>,
    /// Non-fatal #75 unknown-key warnings from layered validation.
    pub warnings: Vec<UnknownConfigKey>,
    /// Rendered migrated TOML, present only in `Preview` mode.
    pub migrated_toml: Option<String>,
}

impl MigrationOutcome {
    /// Outcome for a document that needs no migration.
    pub fn current(mode: MigrationMode, path: PathBuf, version: u32) -> Self {
        Self {
            mode,
            changed: false,
            path,
            from_version: version,
            to_version: version,
            steps: Vec::new(),
            backup_path: None,
            warnings: Vec::new(),
            migrated_toml: None,
        }
    }
}

/// Structured migration error. Every variant carries a stable machine code
/// (see [`MigrationError::code`]) and, where relevant, a filesystem path.
#[derive(Debug)]
pub enum MigrationError {
    ReadFailed {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        message: String,
        span: Option<Range<usize>>,
    },
    /// Negative, non-integer, or out-of-range `version` values.
    InvalidVersion {
        path: PathBuf,
        message: String,
    },
    /// A version newer than the latest supported schema; never downgraded.
    FutureVersion {
        path: PathBuf,
        found: u32,
        latest: u32,
    },
    /// A version older than the oldest registered migration.
    UnsupportedOldVersion {
        path: PathBuf,
        found: u32,
        oldest: u32,
    },
    RegistryGap {
        from: u32,
        to: u32,
    },
    RegistryDuplicate {
        from: u32,
    },
    RegistryNonUnitJump {
        from: u32,
        to: u32,
    },
    RegistryUnreachable {
        latest: u32,
    },
    RegistryExceedsLatest {
        from: u32,
        to: u32,
        latest: u32,
    },
    StepFailed {
        step: String,
        message: String,
    },
    /// Complete layered candidate (or a fragment/override) failed validation.
    CandidateValidation {
        diagnostics: Vec<ConfigDiagnostic>,
    },
    /// A fragment or override carries a `version` key, which belongs only to
    /// the primary document.
    InvalidSourceVersion {
        path: PathBuf,
        version: Option<u64>,
    },
    BackupFailed {
        path: PathBuf,
        source: io::Error,
    },
    WriteFailed {
        path: PathBuf,
        source: io::Error,
    },
    ConcurrentModification {
        path: PathBuf,
    },
    TempCleanupFailed {
        path: PathBuf,
        source: io::Error,
    },
}

impl MigrationError {
    /// Stable machine-readable error code for CLI JSON diagnostics.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ReadFailed { .. } => "config.migration.read_failed",
            Self::Parse { .. } => "config.migration.parse_failed",
            Self::InvalidVersion { .. } => "config.migration.invalid_version",
            Self::FutureVersion { .. } => "config.migration.future_version",
            Self::UnsupportedOldVersion { .. } => "config.migration.unsupported_old_version",
            Self::RegistryGap { .. } => "config.migration.registry_gap",
            Self::RegistryDuplicate { .. } => "config.migration.registry_duplicate",
            Self::RegistryNonUnitJump { .. } => "config.migration.registry_non_unit_jump",
            Self::RegistryUnreachable { .. } => "config.migration.registry_unreachable",
            Self::RegistryExceedsLatest { .. } => "config.migration.registry_exceeds_latest",
            Self::StepFailed { .. } => "config.migration.step_failed",
            Self::CandidateValidation { .. } => "config.migration.validation_failed",
            Self::InvalidSourceVersion { .. } => "config.migration.invalid_source_version",
            Self::BackupFailed { .. } => "config.migration.backup_failed",
            Self::WriteFailed { .. } => "config.migration.write_failed",
            Self::ConcurrentModification { .. } => "config.migration.concurrent_modification",
            Self::TempCleanupFailed { .. } => "config.migration.cleanup_failed",
        }
    }

    /// The filesystem path this error concerns, when known.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::ReadFailed { path, .. }
            | Self::Parse { path, .. }
            | Self::InvalidVersion { path, .. }
            | Self::FutureVersion { path, .. }
            | Self::UnsupportedOldVersion { path, .. }
            | Self::InvalidSourceVersion { path, .. }
            | Self::BackupFailed { path, .. }
            | Self::WriteFailed { path, .. }
            | Self::ConcurrentModification { path }
            | Self::TempCleanupFailed { path, .. } => Some(path),
            _ => None,
        }
    }
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadFailed { path, source } => {
                write!(f, "cannot read config {}: {source}", path.display())
            }
            Self::Parse { path, message, .. } => write!(f, "config {}: {message}", path.display()),
            Self::InvalidVersion { path, message } => {
                write!(f, "config {}: {message}", path.display())
            }
            Self::FutureVersion {
                path,
                found,
                latest,
            } => write!(
                f,
                "config {}: version {found} is newer than the latest supported version {latest}; refusing to downgrade",
                path.display()
            ),
            Self::UnsupportedOldVersion {
                path,
                found,
                oldest,
            } => write!(
                f,
                "config {}: version {found} is older than the oldest supported migration (schema {oldest})",
                path.display()
            ),
            Self::RegistryGap { from, to } => write!(
                f,
                "migration registry has a gap between version {from} and version {to}"
            ),
            Self::RegistryDuplicate { from } => {
                write!(
                    f,
                    "migration registry has duplicate steps from version {from}"
                )
            }
            Self::RegistryNonUnitJump { from, to } => write!(
                f,
                "migration registry step {from} -> {to} must advance by exactly one version"
            ),
            Self::RegistryUnreachable { latest } => write!(
                f,
                "migration registry does not terminate at latest version {latest}"
            ),
            Self::RegistryExceedsLatest { from, to, latest } => write!(
                f,
                "migration registry step {from} -> {to} exceeds the latest version {latest}"
            ),
            Self::StepFailed { step, message } => {
                write!(f, "migration step {step} failed: {message}")
            }
            Self::CandidateValidation { diagnostics } => write!(
                f,
                "migrated config failed validation: {}",
                diagnostics
                    .iter()
                    .map(|d| format!("{}: {}", d.path, d.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Self::InvalidSourceVersion { path, version } => {
                write!(
                    f,
                    "config {}: 'version' belongs only to the primary document",
                    path.display()
                )?;
                match version {
                    Some(version) => write!(f, " (found version {version})"),
                    None => write!(f, " (value must be an integer)"),
                }
            }
            Self::BackupFailed { path, source } => {
                write!(f, "cannot create backup {}: {source}", path.display())
            }
            Self::WriteFailed { path, source } => {
                write!(f, "cannot write config {}: {source}", path.display())
            }
            Self::ConcurrentModification { path } => write!(
                f,
                "config {} changed during migration; refusing to overwrite",
                path.display()
            ),
            Self::TempCleanupFailed { path, source } => write!(
                f,
                "cannot remove temporary file {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for MigrationError {}

/// Minimal filesystem seam so the commit transaction can be exercised with
/// deterministic injected failures in tests.
pub trait MigrationFs: Send + Sync {
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn read_metadata(&self, path: &Path) -> io::Result<fs::Metadata>;
    /// Open with create-new semantics; fails on `AlreadyExists`.
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn FileOps>>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()>;
    fn sync_directory(&self, path: &Path) -> io::Result<()>;
}

/// Open file handle used by the commit transaction.
pub trait FileOps: Send + Sync {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn sync_all(&mut self) -> io::Result<()>;
}

/// The real filesystem implementation of [`MigrationFs`].
#[derive(Clone, Debug, Default)]
pub struct StdFs;

impl MigrationFs for StdFs {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn read_metadata(&self, path: &Path) -> io::Result<fs::Metadata> {
        fs::metadata(path)
    }

    fn create_new(&self, path: &Path) -> io::Result<Box<dyn FileOps>> {
        Ok(Box::new(StdFile(fs::File::create_new(path)?)))
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()> {
        fs::set_permissions(path, permissions)
    }

    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        fs::File::open(path)?.sync_all()
    }
}

struct StdFile(fs::File);

impl FileOps for StdFile {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write_all(bytes)
    }

    fn sync_all(&mut self) -> io::Result<()> {
        self.0.sync_all()
    }
}

/// The migration service shared by shell startup, manual reload guards, and
/// the `shilpo config migrate` CLI command. Explicit paths only; tests never
/// touch process-global XDG variables.
#[derive(Clone)]
pub struct MigrationService {
    pub config_dir: PathBuf,
    pub primary_path: PathBuf,
    registry: MigrationRegistry,
    clock: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
    fs: Arc<dyn MigrationFs>,
}

impl MigrationService {
    pub fn for_primary_path(primary_path: impl AsRef<Path>) -> Self {
        Self::with_parts(
            primary_path.as_ref().to_path_buf(),
            MigrationRegistry::production(),
            Arc::new(Utc::now),
            Arc::new(StdFs),
        )
    }

    /// Service with an injectable clock (deterministic backup timestamps).
    pub fn with_clock(
        primary_path: impl AsRef<Path>,
        clock: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
    ) -> Self {
        Self::with_parts(
            primary_path.as_ref().to_path_buf(),
            MigrationRegistry::production(),
            clock,
            Arc::new(StdFs),
        )
    }

    /// Fully injectable service for tests: registry, clock, and filesystem
    /// seam are all provided by the caller.
    pub fn with_parts(
        primary_path: PathBuf,
        registry: MigrationRegistry,
        clock: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
        fs: Arc<dyn MigrationFs>,
    ) -> Self {
        let config_dir = primary_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Self {
            config_dir,
            primary_path,
            registry,
            clock,
            fs,
        }
    }

    /// Run the migration pipeline in the given mode.
    ///
    /// Order: read -> detect version -> plan -> apply steps in memory ->
    /// validate the complete layered candidate -> (preview stops here) ->
    /// concurrent-modification check -> durable backup -> atomic
    /// same-directory replacement -> directory sync.
    pub fn run(&self, mode: MigrationMode) -> Result<MigrationOutcome, MigrationError> {
        let text = match self.fs.read_to_string(&self.primary_path) {
            Ok(text) => text,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Ok(MigrationOutcome::current(
                    mode,
                    self.primary_path.clone(),
                    self.registry.latest(),
                ));
            }
            Err(source) => {
                return Err(MigrationError::ReadFailed {
                    path: self.primary_path.clone(),
                    source,
                });
            }
        };
        if text.trim().is_empty() {
            return Ok(MigrationOutcome::current(
                mode,
                self.primary_path.clone(),
                self.registry.latest(),
            ));
        }
        let metadata = self
            .fs
            .read_metadata(&self.primary_path)
            .map_err(|source| MigrationError::ReadFailed {
                path: self.primary_path.clone(),
                source,
            })?;

        let doc = text
            .parse::<DocumentMut>()
            .map_err(|error| MigrationError::Parse {
                path: self.primary_path.clone(),
                message: error.to_string(),
                span: error.span(),
            })?;
        let original_version =
            detect_version(&doc, &self.primary_path, self.registry.latest())?.unwrap_or(0);

        // Version metadata belongs only to the primary source. Check this
        // even for an already-current primary so a no-op migration cannot
        // hide an invalid fragment or override.
        self.reject_source_versions()?;

        let plan = self.registry.plan(original_version, &self.primary_path)?;
        if plan.is_empty() {
            return Ok(MigrationOutcome::current(
                mode,
                self.primary_path.clone(),
                self.registry.latest(),
            ));
        }

        // Apply every step in order to an in-memory document only.
        let mut migrated = doc;
        apply_plan(&mut migrated, &plan, &self.primary_path)?;
        let migrated_toml = migrated.to_string();

        // Validate the migrated primary together with the current fragments
        // and overrides before any filesystem write.
        let warnings = self.validate_layered(&migrated_toml)?;

        let mut outcome = MigrationOutcome {
            mode,
            changed: true,
            path: self.primary_path.clone(),
            from_version: original_version,
            to_version: plan.to_version,
            steps: plan
                .steps
                .iter()
                .map(|step| step.name.to_string())
                .collect(),
            backup_path: None,
            warnings,
            migrated_toml: None,
        };

        if mode == MigrationMode::Preview {
            outcome.migrated_toml = Some(migrated_toml);
            return Ok(outcome);
        }

        // Refuse to overwrite a primary that changed since it was read.
        if !self.primary_unchanged(&text, &metadata) {
            return Err(MigrationError::ConcurrentModification {
                path: self.primary_path.clone(),
            });
        }

        let backup_path = self.create_backup(text.as_bytes())?;
        let temp_path = self.write_temp(migrated_toml.as_bytes(), &metadata)?;

        if let Err(source) = self.fs.rename(&temp_path, &self.primary_path) {
            return match self.fs.remove_file(&temp_path) {
                Ok(()) => Err(MigrationError::WriteFailed {
                    path: self.primary_path.clone(),
                    source,
                }),
                Err(cleanup_source) => Err(MigrationError::TempCleanupFailed {
                    path: temp_path,
                    source: cleanup_source,
                }),
            };
        }

        if let Err(source) = self.fs.sync_directory(&self.config_dir) {
            return Err(MigrationError::WriteFailed {
                path: self.config_dir.clone(),
                source,
            });
        }

        outcome.backup_path = Some(backup_path);
        Ok(outcome)
    }

    /// Read-only primary status used by manual reload. Never writes.
    pub fn primary_status(&self) -> Result<PrimaryStatus, MigrationError> {
        let text = match self.fs.read_to_string(&self.primary_path) {
            Ok(text) => text,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Ok(PrimaryStatus::Missing);
            }
            Err(source) => {
                return Err(MigrationError::ReadFailed {
                    path: self.primary_path.clone(),
                    source,
                });
            }
        };
        if text.trim().is_empty() {
            return Ok(PrimaryStatus::Empty);
        }
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|error| MigrationError::Parse {
                path: self.primary_path.clone(),
                message: error.to_string(),
                span: error.span(),
            })?;
        let version =
            detect_version(&doc, &self.primary_path, self.registry.latest())?.unwrap_or(0);
        self.reject_source_versions()?;
        if self.registry.plan(version, &self.primary_path)?.is_empty() {
            Ok(PrimaryStatus::Current)
        } else {
            Ok(PrimaryStatus::NeedsMigration {
                from_version: version,
            })
        }
    }

    /// Re-verify the original primary bytes and stable metadata before the
    /// commit point.
    fn primary_unchanged(&self, original_text: &str, original_metadata: &fs::Metadata) -> bool {
        let Ok(current_text) = self.fs.read_to_string(&self.primary_path) else {
            return false;
        };
        if current_text != original_text {
            return false;
        }
        let Ok(current_metadata) = self.fs.read_metadata(&self.primary_path) else {
            return true;
        };
        if current_metadata.len() != original_metadata.len() {
            return false;
        }
        match (current_metadata.modified(), original_metadata.modified()) {
            (Ok(current), Ok(original)) => current == original,
            _ => true,
        }
    }

    /// Validate the migrated in-memory primary plus the current fragments and
    /// overrides through the resolver's narrow injection hook. Unknown keys
    /// are non-fatal warnings; anything else blocks the migration.
    fn validate_layered(
        &self,
        migrated_toml: &str,
    ) -> Result<Vec<UnknownConfigKey>, MigrationError> {
        self.reject_source_versions()?;
        let resolver = ConfigResolver::from_primary_path(&self.primary_path);
        let (candidate, _provenance, _sources, warnings) = resolver
            .resolve_candidate_with_primary(migrated_toml)
            .map_err(|error| MigrationError::CandidateValidation {
                diagnostics: config_diagnostics(&error),
            })?;
        candidate
            .validate()
            .map_err(|error| MigrationError::CandidateValidation {
                diagnostics: config_diagnostics(&error),
            })?;
        Ok(warnings)
    }

    /// Schema version belongs only to the primary document: a `version` key
    /// in a fragment or override is invalid for that source.
    fn reject_source_versions(&self) -> Result<(), MigrationError> {
        for disc in discover_sources(&self.config_dir, &self.primary_path) {
            if matches!(disc.source, ConfigSource::Primary { .. }) {
                continue;
            }
            let (doc, _text) = read_source_document(&disc.path).map_err(|error| {
                MigrationError::CandidateValidation {
                    diagnostics: config_diagnostics(&error),
                }
            })?;
            if let Some(item) = doc.get("version") {
                let version = match item {
                    Item::Value(Value::Integer(integer)) if *integer.value() >= 0 => {
                        Some(*integer.value() as u64)
                    }
                    _ => None,
                };
                return Err(MigrationError::InvalidSourceVersion {
                    path: disc.path,
                    version,
                });
            }
        }
        Ok(())
    }

    /// Create a durable byte-for-byte backup with create-new semantics.
    /// On collision, a deterministic numeric suffix is appended; an existing
    /// backup is never overwritten.
    fn create_backup(&self, original: &[u8]) -> Result<PathBuf, MigrationError> {
        let timestamp = (self.clock)().format("%Y%m%dT%H%M%S%.6fZ").to_string();
        let mut candidate = self
            .primary_path
            .with_file_name(format!("config.toml.bak.{timestamp}"));
        let mut suffix = 0usize;
        loop {
            let mut file = match self.fs.create_new(&candidate) {
                Ok(file) => file,
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                    suffix += 1;
                    candidate = self
                        .primary_path
                        .with_file_name(format!("config.toml.bak.{timestamp}.{suffix}"));
                    continue;
                }
                Err(source) => {
                    return Err(MigrationError::BackupFailed {
                        path: candidate,
                        source,
                    });
                }
            };
            file.write_all(original)
                .map_err(|source| MigrationError::BackupFailed {
                    path: candidate.clone(),
                    source,
                })?;
            file.sync_all()
                .map_err(|source| MigrationError::BackupFailed {
                    path: candidate.clone(),
                    source,
                })?;
            return Ok(candidate);
        }
    }

    /// Write the migrated bytes to a same-directory temp file (create-new,
    /// process ID plus collision-resistant UUID suffix), flush, sync, and
    /// apply the original permissions. The primary is only replaced by the
    /// subsequent rename.
    fn write_temp(&self, bytes: &[u8], metadata: &fs::Metadata) -> Result<PathBuf, MigrationError> {
        for _ in 0..8 {
            let temp_path = self.primary_path.with_file_name(format!(
                "config.toml.{}.{}.tmp",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            let mut file = match self.fs.create_new(&temp_path) {
                Ok(file) => file,
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    let _ = self.fs.remove_file(&temp_path);
                    return Err(MigrationError::WriteFailed {
                        path: temp_path,
                        source,
                    });
                }
            };
            if let Err(source) = file.write_all(bytes) {
                let _ = self.fs.remove_file(&temp_path);
                return Err(MigrationError::WriteFailed {
                    path: temp_path,
                    source,
                });
            }
            if let Err(source) = file.sync_all() {
                let _ = self.fs.remove_file(&temp_path);
                return Err(MigrationError::WriteFailed {
                    path: temp_path,
                    source,
                });
            }
            if let Err(source) = self.fs.set_permissions(&temp_path, metadata.permissions()) {
                let _ = self.fs.remove_file(&temp_path);
                return Err(MigrationError::WriteFailed {
                    path: temp_path,
                    source,
                });
            }
            return Ok(temp_path);
        }
        Err(MigrationError::WriteFailed {
            path: self.primary_path.clone(),
            source: io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a unique temporary file",
            ),
        })
    }
}

/// Read the top-level `version` of a parsed document.
///
/// - `None` (missing key) means legacy schema 0.
/// - `Some(0)` is also schema 0.
/// - Values above `latest` are a hard future-version error.
/// - Negative, non-integer, and out-of-range values are invalid diagnostics.
pub fn detect_version(
    doc: &DocumentMut,
    path: &Path,
    latest: u32,
) -> Result<Option<u32>, MigrationError> {
    let Some(item) = doc.get("version") else {
        return Ok(None);
    };
    match item {
        Item::Value(Value::Integer(integer)) => {
            let value = *integer.value();
            if value < 0 {
                return Err(MigrationError::InvalidVersion {
                    path: path.to_path_buf(),
                    message: format!("'version' must be a non-negative integer, found {value}"),
                });
            }
            let value = value as u64;
            if value > u32::MAX as u64 {
                return Err(MigrationError::InvalidVersion {
                    path: path.to_path_buf(),
                    message: format!("'version' value {value} is out of range"),
                });
            }
            let version = value as u32;
            if version > latest {
                return Err(MigrationError::FutureVersion {
                    path: path.to_path_buf(),
                    found: version,
                    latest,
                });
            }
            Ok(Some(version))
        }
        _ => Err(MigrationError::InvalidVersion {
            path: path.to_path_buf(),
            message: "top-level 'version' must be an integer".to_string(),
        }),
    }
}

/// Run every plan step in order, verifying the document's top-level version
/// after each step. The pipeline stops on the first failure and never writes
/// a partially migrated document.
fn apply_plan(
    doc: &mut DocumentMut,
    plan: &MigrationPlan<'_>,
    path: &Path,
) -> Result<(), MigrationError> {
    for step in &plan.steps {
        (step.apply)(doc).map_err(|message| MigrationError::StepFailed {
            step: step.name.to_string(),
            message,
        })?;
        let version = detect_version(doc, path, plan.to_version)?;
        if version != Some(step.to) {
            return Err(MigrationError::StepFailed {
                step: step.name.to_string(),
                message: format!(
                    "expected top-level 'version' {} after step, found {}",
                    step.to,
                    version.unwrap_or(0)
                ),
            });
        }
    }
    Ok(())
}

/// The real v0 -> v1 migration.
///
/// Inserts `version = 1` when missing (rendered before all table headers by
/// toml_edit) or replaces an explicit `version = 0` in place, preserving the
/// value's decorations (e.g. a leading comment on the version line). Every
/// other byte-equivalent decoration is untouched.
fn v0_to_v1(doc: &mut DocumentMut) -> Result<(), String> {
    set_version(doc, 1)
}

/// Set the top-level version, preserving decorations of an existing entry.
fn set_version(doc: &mut DocumentMut, to: u32) -> Result<(), String> {
    match doc.get_mut("version") {
        Some(Item::Value(value)) => {
            let decor = value.decor().clone();
            let mut new_value = Value::from(to as i64);
            *new_value.decor_mut() = decor;
            *value = new_value;
            Ok(())
        }
        Some(_) => Err("top-level 'version' must be an integer value".to_string()),
        None => {
            doc.insert("version", Item::Value(Value::from(to as i64)));
            Ok(())
        }
    }
}

/// Flatten a resolver error into candidate-validation diagnostics.
fn config_diagnostics(error: &ConfigError) -> Vec<ConfigDiagnostic> {
    match error {
        ConfigError::Validation { diagnostics } => diagnostics.clone(),
        ConfigError::Parse { diagnostic } => vec![diagnostic.clone()],
        ConfigError::Io { path, source } => {
            vec![ConfigDiagnostic::new(
                path.display().to_string(),
                source.to_string(),
            )]
        }
        ConfigError::Serialize { source } => {
            vec![ConfigDiagnostic::new("config", source.to_string())]
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use tempfile::TempDir;

    use super::*;

    fn temp_dir() -> TempDir {
        TempDir::new().expect("temp dir")
    }

    fn write_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
        path
    }

    fn primary_path(dir: &TempDir) -> PathBuf {
        dir.path().join("config.toml")
    }

    fn fixed_clock() -> Arc<dyn Fn() -> DateTime<Utc> + Send + Sync> {
        Arc::new(|| {
            DateTime::parse_from_rfc3339("2026-01-02T03:04:05.123456Z")
                .unwrap()
                .with_timezone(&Utc)
        })
    }

    fn file_names(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn assert_no_backup_or_temp(dir: &Path) {
        let names = file_names(dir);
        assert!(
            !names
                .iter()
                .any(|name| name.contains(".bak.") || name.ends_with(".tmp")),
            "unexpected backup/temp files: {names:?}"
        );
    }

    // ------------------------------------------------------------------
    // Version and registry
    // ------------------------------------------------------------------

    #[test]
    fn test_migration_missing_version_plans_v0_to_v1() {
        let registry = MigrationRegistry::production();
        let plan = registry.plan(0, Path::new("config.toml")).unwrap();
        assert_eq!(plan.from_version, 0);
        assert_eq!(plan.to_version, 1);
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].name, "v0 -> v1");
    }

    #[test]
    fn test_migration_missing_version_migrates_to_v1() {
        let dir = temp_dir();
        let primary = write_file(
            &dir,
            "config.toml",
            "[theme]\nfont_family = \"sans-serif\"\n",
        );
        let outcome = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Preview)
            .unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.from_version, 0);
        assert_eq!(outcome.to_version, 1);
        assert_eq!(outcome.steps, vec!["v0 -> v1"]);
        assert!(outcome.migrated_toml.unwrap().starts_with("version = 1"));
    }

    #[test]
    fn test_migration_explicit_version_zero_migrates_to_v1() {
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", "version = 0\n\n[bar]\nheight = 48\n");
        let outcome = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Preview)
            .unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.from_version, 0);
        assert_eq!(outcome.to_version, 1);
    }

    #[test]
    fn test_migration_current_version_is_noop_without_writes() {
        let dir = temp_dir();
        let primary = write_file(
            &dir,
            "config.toml",
            "version = 1\n\n[theme]\nfont_family = \"sans-serif\"\n",
        );
        let original = fs::read(&primary).unwrap();
        let service = MigrationService::for_primary_path(&primary);

        let preview = service.run(MigrationMode::Preview).unwrap();
        assert!(!preview.changed);
        assert_eq!(preview.steps, Vec::<String>::new());
        assert!(preview.migrated_toml.is_none());

        let applied = service.run(MigrationMode::Apply).unwrap();
        assert!(!applied.changed);
        assert!(applied.backup_path.is_none());

        assert_eq!(fs::read(&primary).unwrap(), original);
        assert_no_backup_or_temp(dir.path());
    }

    #[test]
    fn test_migration_future_version_is_hard_error() {
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", "version = 9999\n");
        let error = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap_err();
        match error {
            MigrationError::FutureVersion { found, latest, .. } => {
                assert_eq!(found, 9999);
                assert_eq!(latest, LATEST_CONFIG_VERSION);
            }
            other => panic!("expected FutureVersion, got {other:?}"),
        }
        assert_eq!(error.code(), "config.migration.future_version");
        assert_eq!(fs::read_to_string(&primary).unwrap(), "version = 9999\n");
        assert_no_backup_or_temp(dir.path());
    }

    #[test]
    fn test_migration_invalid_version_values_are_distinct_errors() {
        let cases = [
            ("version = -1\n", "config.migration.invalid_version"),
            ("version = \"one\"\n", "config.migration.invalid_version"),
            ("version = 1.0\n", "config.migration.invalid_version"),
            ("version = 4294967296\n", "config.migration.invalid_version"),
            ("version = 2\n", "config.migration.future_version"),
        ];
        for (content, expected_code) in cases {
            let dir = temp_dir();
            let primary = write_file(&dir, "config.toml", content);
            let error = MigrationService::for_primary_path(&primary)
                .run(MigrationMode::Apply)
                .unwrap_err();
            assert_eq!(error.code(), expected_code, "content: {content}");
            assert_eq!(fs::read_to_string(&primary).unwrap(), content);
            assert_no_backup_or_temp(dir.path());
        }
    }

    #[test]
    fn test_migration_unsupported_old_version_error() {
        let registry = MigrationRegistry::new(
            vec![Migration::new(1, 2, "v1 -> v2", |doc| set_version(doc, 2))],
            2,
        )
        .unwrap();
        let error = registry.plan(0, Path::new("config.toml")).unwrap_err();
        match error {
            MigrationError::UnsupportedOldVersion { found, oldest, .. } => {
                assert_eq!(found, 0);
                assert_eq!(oldest, 1);
            }
            other => panic!("expected UnsupportedOldVersion, got {other:?}"),
        }
    }

    #[test]
    fn test_migration_registry_rejects_gaps_duplicates_non_unit_and_unreachable() {
        let gap = MigrationRegistry::new(
            vec![
                Migration::new(0, 1, "v0 -> v1", |doc| set_version(doc, 1)),
                Migration::new(2, 3, "v2 -> v3", |doc| set_version(doc, 3)),
            ],
            3,
        )
        .unwrap_err();
        assert!(matches!(
            gap,
            MigrationError::RegistryGap { from: 1, to: 2 }
        ));

        let duplicate = MigrationRegistry::new(
            vec![
                Migration::new(0, 1, "v0 -> v1", |doc| set_version(doc, 1)),
                Migration::new(0, 1, "v0 -> v1 again", |doc| set_version(doc, 1)),
            ],
            1,
        )
        .unwrap_err();
        assert!(matches!(
            duplicate,
            MigrationError::RegistryDuplicate { from: 0 }
        ));

        let non_unit = MigrationRegistry::new(
            vec![Migration::new(0, 2, "v0 -> v2", |doc| set_version(doc, 2))],
            2,
        )
        .unwrap_err();
        assert!(matches!(
            non_unit,
            MigrationError::RegistryNonUnitJump { from: 0, to: 2 }
        ));

        let unreachable = MigrationRegistry::new(
            vec![Migration::new(0, 1, "v0 -> v1", |doc| set_version(doc, 1))],
            2,
        )
        .unwrap_err();
        assert!(matches!(
            unreachable,
            MigrationError::RegistryUnreachable { latest: 2 }
        ));

        let exceeds = MigrationRegistry::new(
            vec![
                Migration::new(0, 1, "v0 -> v1", |doc| set_version(doc, 1)),
                Migration::new(1, 2, "v1 -> v2", |doc| set_version(doc, 2)),
            ],
            1,
        )
        .unwrap_err();
        assert!(matches!(
            exceeds,
            MigrationError::RegistryExceedsLatest { latest: 1, .. }
        ));
    }

    /// Test-only synthetic step that renames a top-level key in place,
    /// preserving the value, the key decorations (e.g. a leading comment),
    /// and the scalar ordering imposed by the TOML serializer.
    fn rename_key(doc: &mut DocumentMut, from: &str, to: &str) -> Result<(), String> {
        let (old_key, item) = doc
            .as_table_mut()
            .remove_entry(from)
            .ok_or_else(|| format!("missing key '{from}'"))?;
        let mut new_key = toml_edit::Key::new(to);
        *new_key.leaf_decor_mut() = old_key.leaf_decor().clone();
        doc.as_table_mut().insert_formatted(&new_key, item);
        Ok(())
    }

    fn synthetic_multi_step_registry() -> MigrationRegistry {
        MigrationRegistry::new(
            vec![
                Migration::new(0, 1, "v0 -> v1", v0_to_v1),
                Migration::new(1, 2, "v1 -> v2 rename", |doc| {
                    rename_key(doc, "legacy_name", "modern_name")?;
                    set_version(doc, 2)
                }),
            ],
            2,
        )
        .expect("synthetic registry invariants")
    }

    #[test]
    fn test_migration_synthetic_multi_step_runs_in_order() {
        let dir = temp_dir();
        let primary = write_file(
            &dir,
            "config.toml",
            "legacy_name = \"alice\"\n\n[theme]\nfont_family = \"sans-serif\"\n",
        );
        let service = MigrationService::with_parts(
            primary.clone(),
            synthetic_multi_step_registry(),
            Arc::new(Utc::now),
            Arc::new(StdFs),
        );
        // The synthetic registry terminates at schema 2, which the typed
        // ShellConfig cannot validate; prove ordered composition through the
        // plan/apply pipeline directly.
        let plan = service
            .registry
            .plan(0, &primary)
            .expect("plan for synthetic chain");
        assert_eq!(
            plan.steps.iter().map(|step| step.name).collect::<Vec<_>>(),
            vec!["v0 -> v1", "v1 -> v2 rename"]
        );
        let mut doc: DocumentMut = fs::read_to_string(&primary).unwrap().parse().unwrap();
        apply_plan(&mut doc, &plan, &primary).unwrap();
        let rendered = doc.to_string();
        assert!(rendered.contains("modern_name = \"alice\""), "{rendered}");
        assert!(!rendered.contains("legacy_name"), "{rendered}");
        assert_eq!(detect_version(&doc, &primary, 2).unwrap(), Some(2));
    }

    #[test]
    fn test_migration_synthetic_rename_preserves_value_and_decorations() {
        let dir = temp_dir();
        let primary = write_file(
            &dir,
            "config.toml",
            "# the legacy setting\nlegacy_name = \"alice\"\n\n[theme]\nfont_family = \"sans-serif\"\n",
        );
        let registry = synthetic_multi_step_registry();
        let plan = registry.plan(0, &primary).unwrap();
        let mut doc: DocumentMut = fs::read_to_string(&primary).unwrap().parse().unwrap();
        apply_plan(&mut doc, &plan, &primary).unwrap();
        let rendered = doc.to_string();
        // The leading comment and the renamed value survive together.
        assert!(rendered.contains("# the legacy setting"), "{rendered}");
        assert!(rendered.contains("modern_name = \"alice\""), "{rendered}");
        assert!(!rendered.contains("legacy_name"), "{rendered}");
    }

    // ------------------------------------------------------------------
    // Preservation and validation
    // ------------------------------------------------------------------

    #[test]
    fn test_migration_preserves_comments_formatting_and_key_order() {
        let original = concat!(
            "# Shilpo configuration\n",
            "\n",
            "[theme]\n",
            "\"font_family\" = \"sans-serif\"\n",
            "# array with spacing survives\n",
            "widgets = [\"a\", \"b\", \"c\"]\n",
            "inline = { x = 1, y = 2 }\n",
            "\n",
            "[bar]\n",
            "height = 48\n",
        );
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", original);
        let outcome = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Preview)
            .unwrap();
        let migrated = outcome.migrated_toml.expect("preview toml");
        let lines: Vec<&str> = migrated.lines().collect();
        assert_eq!(lines[0], "version = 1");
        let rest = lines[1..].join("\n") + "\n";
        assert_eq!(rest, original, "non-version bytes must be untouched");
    }

    #[test]
    fn test_migration_explicit_zero_replaced_in_place() {
        let original = concat!(
            "version = 0\n",
            "\n",
            "# comment survives\n",
            "[theme]\n",
            "font_family = \"sans-serif\"\n",
        );
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", original);
        let outcome = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Preview)
            .unwrap();
        let migrated = outcome.migrated_toml.unwrap();
        assert_eq!(migrated, original.replacen("version = 0", "version = 1", 1));
    }

    #[test]
    fn test_migration_layered_validation_uses_migrated_primary_with_sources() {
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", "[bar]\nheight = 48\n");
        write_file(
            &dir,
            "conf.d/01-theme.toml",
            "[theme]\nfont_family = \"fragment-font\"\n",
        );
        write_file(&dir, "overrides.toml", "[bar]\nwidget_spacing = 6\n");
        let outcome = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap();
        assert!(outcome.changed);
        assert!(outcome.warnings.is_empty());

        // After migration the complete layered candidate resolves with the
        // fragment and override winning at their layers.
        let resolver = ConfigResolver::from_primary_path(&primary);
        let (snapshot, _report) = resolver.resolve_initial().unwrap();
        assert_eq!(snapshot.config.theme.font_family, "fragment-font");
        assert_eq!(snapshot.config.bar.widget_spacing, 6);
    }

    #[test]
    fn test_migration_blocking_fragment_error_causes_zero_writes() {
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", "[bar]\nheight = 48\n");
        write_file(&dir, "conf.d/01-bad.toml", "bar = \"not a table\"\n");
        let error = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap_err();
        assert!(matches!(error, MigrationError::CandidateValidation { .. }));
        assert_eq!(
            fs::read_to_string(&primary).unwrap(),
            "[bar]\nheight = 48\n"
        );
        assert_no_backup_or_temp(dir.path());
    }

    #[test]
    fn test_migration_unknown_keys_are_warnings_and_preserved_in_file() {
        let dir = temp_dir();
        let primary = write_file(
            &dir,
            "config.toml",
            "# typo preserved\n[bar]\nheigth = 44\nheight = 48\n",
        );
        let outcome = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(outcome.warnings[0].path, "bar.heigth");
        assert!(outcome.warnings[0].suggestion.is_some());
        // The unknown key stays in the user file; only in-memory resolution
        // drops it.
        let migrated = fs::read_to_string(&primary).unwrap();
        assert!(migrated.contains("heigth = 44"), "{migrated}");
        assert!(migrated.starts_with("version = 1"), "{migrated}");
    }

    #[test]
    fn test_migration_leaves_fragments_and_overrides_byte_unchanged() {
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", "[bar]\nheight = 48\n");
        let fragment = write_file(
            &dir,
            "conf.d/01-theme.toml",
            "[theme]\nfont_family = \"fragment-font\"\n",
        );
        let overrides = write_file(&dir, "overrides.toml", "[bar]\nwidget_spacing = 6\n");
        let fragment_before = fs::read(&fragment).unwrap();
        let overrides_before = fs::read(&overrides).unwrap();

        MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap();

        assert_eq!(fs::read(&fragment).unwrap(), fragment_before);
        assert_eq!(fs::read(&overrides).unwrap(), overrides_before);
    }

    #[test]
    fn test_migration_rejects_version_in_fragments_and_overrides() {
        for (name, content) in [
            ("conf.d/01-bad.toml", "version = 1\n[bar]\nheight = 48\n"),
            ("overrides.toml", "version = 1\n"),
        ] {
            let dir = temp_dir();
            let primary = write_file(&dir, "config.toml", "[bar]\nheight = 48\n");
            write_file(&dir, name, content);
            let error = MigrationService::for_primary_path(&primary)
                .run(MigrationMode::Apply)
                .unwrap_err();
            match error {
                MigrationError::InvalidSourceVersion { path, version } => {
                    assert_eq!(
                        path.file_name().unwrap().to_str().unwrap(),
                        name.rsplit('/').next().unwrap()
                    );
                    assert_eq!(version, Some(1));
                }
                other => panic!("expected InvalidSourceVersion for {name}, got {other:?}"),
            }
            assert_eq!(
                fs::read_to_string(&primary).unwrap(),
                "[bar]\nheight = 48\n"
            );
            assert_no_backup_or_temp(dir.path());
        }
    }

    #[test]
    fn test_current_primary_still_rejects_version_in_fragments() {
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", "version = 1\n");
        write_file(&dir, "conf.d/01-bad.toml", "version = 1\n");

        let error = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap_err();
        assert!(matches!(error, MigrationError::InvalidSourceVersion { .. }));
        assert_eq!(fs::read_to_string(&primary).unwrap(), "version = 1\n");
        assert_no_backup_or_temp(dir.path());
    }

    #[test]
    fn test_migration_invalid_migrated_candidate_produces_zero_writes() {
        let dir = temp_dir();
        let primary = write_file(
            &dir,
            "config.toml",
            "# comment\n[bar]\nheight = \"not-a-number\"\n",
        );
        let error = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap_err();
        assert!(matches!(error, MigrationError::CandidateValidation { .. }));
        assert_no_backup_or_temp(dir.path());
        assert_eq!(
            fs::read_to_string(&primary).unwrap(),
            "# comment\n[bar]\nheight = \"not-a-number\"\n"
        );
    }

    // ------------------------------------------------------------------
    // Filesystem transaction
    // ------------------------------------------------------------------

    #[test]
    fn test_migration_dry_run_is_read_only() {
        let dir = temp_dir();
        let original = "# c\n[theme]\nfont_family = \"sans-serif\"\n";
        let primary = write_file(&dir, "config.toml", original);
        let outcome = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Preview)
            .unwrap();
        assert!(outcome.changed);
        assert!(outcome.migrated_toml.is_some());
        assert!(outcome.backup_path.is_none());
        assert_eq!(fs::read_to_string(&primary).unwrap(), original);
        assert_no_backup_or_temp(dir.path());
    }

    #[test]
    fn test_migration_apply_creates_exact_backup_and_migrated_primary() {
        let dir = temp_dir();
        let original = "[theme]\nfont_family = \"sans-serif\"\n\n[bar]\nheight = 48\n";
        let primary = write_file(&dir, "config.toml", original);

        let preview = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Preview)
            .unwrap();
        let expected = preview.migrated_toml.unwrap();

        let applied = MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap();
        assert!(applied.changed);
        let backup_path = applied.backup_path.expect("backup after apply");
        assert_eq!(fs::read(&backup_path).unwrap(), original.as_bytes());
        assert_eq!(fs::read_to_string(&primary).unwrap(), expected);
    }

    #[test]
    fn test_migration_backup_collision_suffix_never_overwrites() {
        let dir = temp_dir();
        let original = "[theme]\nfont_family = \"sans-serif\"\n";
        let primary = write_file(&dir, "config.toml", original);
        let collision = dir.path().join("config.toml.bak.20260102T030405.123456Z");
        let collision_bytes = b"pre-existing backup";
        fs::write(&collision, collision_bytes).unwrap();

        let service = MigrationService::with_clock(primary.clone(), fixed_clock());
        let applied = service.run(MigrationMode::Apply).unwrap();
        let backup_path = applied.backup_path.expect("backup with suffixed name");
        assert_eq!(
            backup_path.file_name().unwrap().to_str().unwrap(),
            "config.toml.bak.20260102T030405.123456Z.1"
        );
        assert_eq!(fs::read(&backup_path).unwrap(), original.as_bytes());
        assert_eq!(fs::read(&collision).unwrap(), collision_bytes);
    }

    #[cfg(unix)]
    #[test]
    fn test_migration_replacement_preserves_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let primary = write_file(
            &dir,
            "config.toml",
            "[theme]\nfont_family = \"sans-serif\"\n",
        );
        fs::set_permissions(&primary, fs::Permissions::from_mode(0o640)).unwrap();
        MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap();
        let mode = fs::metadata(&primary).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o640);
    }

    // --- failure injection -------------------------------------------------

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FailPoint {
        Read,
        Metadata,
        BackupCreate,
        BackupWrite,
        BackupSync,
        TempCreate,
        TempWrite,
        TempSync,
        SetPermissions,
        Rename,
        SyncDirectory,
    }

    struct FailingFile {
        file: fs::File,
        fail_write: bool,
        fail_sync: bool,
    }

    impl FileOps for FailingFile {
        fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
            if self.fail_write {
                return Err(io::Error::other("injected write failure"));
            }
            self.file.write_all(bytes)
        }

        fn sync_all(&mut self) -> io::Result<()> {
            if self.fail_sync {
                return Err(io::Error::other("injected sync failure"));
            }
            self.file.sync_all()
        }
    }

    #[derive(Default)]
    struct FailingFs {
        fail: Option<FailPoint>,
        notes: Mutex<Vec<String>>,
    }

    impl FailingFs {
        fn with(fail: FailPoint) -> Self {
            Self {
                fail: Some(fail),
                notes: Mutex::new(Vec::new()),
            }
        }

        fn note(&self, op: &str, path: &Path) {
            self.notes
                .lock()
                .unwrap()
                .push(format!("{op}:{}", path.display()));
        }

        fn is_backup(path: &Path) -> bool {
            path.file_name()
                .map(|name| name.to_string_lossy().contains(".bak."))
                .unwrap_or(false)
        }
    }

    impl MigrationFs for FailingFs {
        fn read_to_string(&self, path: &Path) -> io::Result<String> {
            self.note("read", path);
            if self.fail == Some(FailPoint::Read) {
                return Err(io::Error::other("injected read failure"));
            }
            fs::read_to_string(path)
        }

        fn read_metadata(&self, path: &Path) -> io::Result<fs::Metadata> {
            self.note("metadata", path);
            if self.fail == Some(FailPoint::Metadata) {
                return Err(io::Error::other("injected metadata failure"));
            }
            fs::metadata(path)
        }

        fn create_new(&self, path: &Path) -> io::Result<Box<dyn FileOps>> {
            self.note("create", path);
            let backup = Self::is_backup(path);
            let fail_point = if backup {
                FailPoint::BackupCreate
            } else {
                FailPoint::TempCreate
            };
            if self.fail == Some(fail_point) {
                return Err(io::Error::other("injected create failure"));
            }
            let file = fs::File::create_new(path)?;
            Ok(Box::new(FailingFile {
                file,
                fail_write: if backup {
                    self.fail == Some(FailPoint::BackupWrite)
                } else {
                    self.fail == Some(FailPoint::TempWrite)
                },
                fail_sync: if backup {
                    self.fail == Some(FailPoint::BackupSync)
                } else {
                    self.fail == Some(FailPoint::TempSync)
                },
            }))
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.note("rename", to);
            if self.fail == Some(FailPoint::Rename) {
                return Err(io::Error::other("injected rename failure"));
            }
            fs::rename(from, to)
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.note("remove", path);
            fs::remove_file(path)
        }

        fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()> {
            self.note("chmod", path);
            if self.fail == Some(FailPoint::SetPermissions) {
                return Err(io::Error::other("injected chmod failure"));
            }
            fs::set_permissions(path, permissions)
        }

        fn sync_directory(&self, path: &Path) -> io::Result<()> {
            self.note("sync_dir", path);
            if self.fail == Some(FailPoint::SyncDirectory) {
                return Err(io::Error::other("injected directory sync failure"));
            }
            fs::File::open(path)?.sync_all()
        }
    }

    #[test]
    fn test_migration_injected_failures_never_replace_primary_before_commit() {
        let original = "[theme]\nfont_family = \"sans-serif\"\n";
        let cases = [
            (FailPoint::Read, "config.migration.read_failed"),
            (FailPoint::Metadata, "config.migration.read_failed"),
            (FailPoint::BackupCreate, "config.migration.backup_failed"),
            (FailPoint::BackupWrite, "config.migration.backup_failed"),
            (FailPoint::BackupSync, "config.migration.backup_failed"),
            (FailPoint::TempCreate, "config.migration.write_failed"),
            (FailPoint::TempWrite, "config.migration.write_failed"),
            (FailPoint::TempSync, "config.migration.write_failed"),
            (FailPoint::SetPermissions, "config.migration.write_failed"),
            (FailPoint::Rename, "config.migration.write_failed"),
        ];
        for (fail_point, expected_code) in cases {
            let dir = temp_dir();
            let primary = write_file(&dir, "config.toml", original);
            let service = MigrationService::with_parts(
                primary.clone(),
                MigrationRegistry::production(),
                Arc::new(Utc::now),
                Arc::new(FailingFs::with(fail_point)),
            );
            let error = service.run(MigrationMode::Apply).unwrap_err();
            assert_eq!(error.code(), expected_code, "{fail_point:?}");
            assert_eq!(
                fs::read_to_string(&primary).unwrap(),
                original,
                "primary must be untouched after {fail_point:?}"
            );
            assert!(
                !file_names(dir.path())
                    .iter()
                    .any(|name| name.ends_with(".tmp")),
                "no orphan temp after {fail_point:?}: {:?}",
                file_names(dir.path())
            );
        }
    }

    #[test]
    fn test_migration_directory_sync_failure_reports_after_commit() {
        // The rename is the commit point; a directory sync failure after it
        // reports WriteFailed but the migrated primary is already in place.
        let dir = temp_dir();
        let primary = write_file(
            &dir,
            "config.toml",
            "[theme]\nfont_family = \"sans-serif\"\n",
        );
        let service = MigrationService::with_parts(
            primary.clone(),
            MigrationRegistry::production(),
            Arc::new(Utc::now),
            Arc::new(FailingFs::with(FailPoint::SyncDirectory)),
        );
        let error = service.run(MigrationMode::Apply).unwrap_err();
        assert_eq!(error.code(), "config.migration.write_failed");
        assert!(
            fs::read_to_string(&primary)
                .unwrap()
                .starts_with("version = 1")
        );
    }

    /// Wrapper that simulates a concurrent editor rewriting the primary
    /// between the service's read and its commit-time re-verification.
    struct MutatingFs {
        primary: PathBuf,
        reads: AtomicUsize,
    }

    impl MigrationFs for MutatingFs {
        fn read_to_string(&self, path: &Path) -> io::Result<String> {
            let n = self.reads.fetch_add(1, Ordering::SeqCst);
            if n == 1 && path == self.primary {
                let mut text = fs::read_to_string(path)?;
                text.push_str("# concurrent edit\n");
                fs::write(path, &text)?;
            }
            fs::read_to_string(path)
        }

        fn read_metadata(&self, path: &Path) -> io::Result<fs::Metadata> {
            fs::metadata(path)
        }

        fn create_new(&self, path: &Path) -> io::Result<Box<dyn FileOps>> {
            let file = fs::File::create_new(path)?;
            Ok(Box::new(FailingFile {
                file,
                fail_write: false,
                fail_sync: false,
            }))
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            fs::rename(from, to)
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            fs::remove_file(path)
        }

        fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()> {
            fs::set_permissions(path, permissions)
        }

        fn sync_directory(&self, path: &Path) -> io::Result<()> {
            fs::File::open(path)?.sync_all()
        }
    }

    #[test]
    fn test_migration_concurrent_modification_is_detected_and_not_overwritten() {
        let dir = temp_dir();
        let primary = write_file(
            &dir,
            "config.toml",
            "[theme]\nfont_family = \"sans-serif\"\n",
        );
        let service = MigrationService::with_parts(
            primary.clone(),
            MigrationRegistry::production(),
            Arc::new(Utc::now),
            Arc::new(MutatingFs {
                primary: primary.clone(),
                reads: AtomicUsize::new(0),
            }),
        );
        let error = service.run(MigrationMode::Apply).unwrap_err();
        assert!(matches!(
            error,
            MigrationError::ConcurrentModification { .. }
        ));
        assert_eq!(error.code(), "config.migration.concurrent_modification");
        // The concurrent editor's content is preserved; the migration did not
        // clobber it with the migrated document.
        let content = fs::read_to_string(&primary).unwrap();
        assert!(content.contains("# concurrent edit"), "{content}");
        assert!(!content.starts_with("version = 1"), "{content}");
        assert!(
            !file_names(dir.path())
                .iter()
                .any(|name| name.ends_with(".tmp")),
            "no orphan temp after concurrent modification"
        );
    }

    // ------------------------------------------------------------------
    // Startup / reload integration
    // ------------------------------------------------------------------

    #[test]
    fn test_migration_startup_helper_applies_before_resolution() {
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", "[bar]\nheight = 48\n");
        let outcome = migrate_primary_for_startup(&primary).unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.from_version, 0);
        assert_eq!(outcome.to_version, LATEST_CONFIG_VERSION);
        assert!(
            fs::read_to_string(&primary)
                .unwrap()
                .starts_with("version = 1")
        );
    }

    #[test]
    fn test_migration_startup_helper_current_is_noop() {
        let dir = temp_dir();
        let original = "version = 1\n\n[bar]\nheight = 48\n";
        let primary = write_file(&dir, "config.toml", original);
        let outcome = migrate_primary_for_startup(&primary).unwrap();
        assert!(!outcome.changed);
        assert_eq!(outcome.backup_path, None);
        assert_eq!(fs::read_to_string(&primary).unwrap(), original);
    }

    #[test]
    fn test_migration_startup_helper_missing_primary_creates_nothing() {
        let dir = temp_dir();
        let primary = primary_path(&dir);
        let outcome = migrate_primary_for_startup(&primary).unwrap();
        assert!(!outcome.changed);
        assert!(!primary.exists());
        assert_no_backup_or_temp(dir.path());
    }

    #[test]
    fn test_migration_startup_helper_reports_migration_error() {
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", "version = 9999\n");
        let error = migrate_primary_for_startup(&primary).unwrap_err();
        assert!(matches!(error, MigrationError::FutureVersion { .. }));
        assert_eq!(fs::read_to_string(&primary).unwrap(), "version = 9999\n");
    }

    #[test]
    fn test_migration_primary_status_read_only_variants() {
        let dir = temp_dir();

        let missing = primary_path(&dir);
        let service = MigrationService::for_primary_path(&missing);
        assert_eq!(service.primary_status().unwrap(), PrimaryStatus::Missing);
        assert_eq!(reload_block_reason(&PrimaryStatus::Missing, &missing), None);

        let empty = write_file(&dir, "config.toml", "   \n");
        let service = MigrationService::for_primary_path(&empty);
        assert_eq!(service.primary_status().unwrap(), PrimaryStatus::Empty);
        assert_eq!(reload_block_reason(&PrimaryStatus::Empty, &empty), None);

        let current = write_file(&dir, "config.toml", "version = 1\n");
        let service = MigrationService::for_primary_path(&current);
        assert_eq!(service.primary_status().unwrap(), PrimaryStatus::Current);
        assert_eq!(reload_block_reason(&PrimaryStatus::Current, &current), None);
    }

    #[test]
    fn test_migration_reload_blocked_for_old_version_without_writing() {
        let dir = temp_dir();
        let original = "[bar]\nheight = 48\n";
        let primary = write_file(&dir, "config.toml", original);
        let service = MigrationService::for_primary_path(&primary);
        let status = service.primary_status().unwrap();
        assert_eq!(status, PrimaryStatus::NeedsMigration { from_version: 0 });
        let reason = reload_block_reason(&status, &primary).expect("block reason");
        assert!(reason.contains("shilpo config migrate"), "{reason}");
        assert!(reason.contains("version 0"), "{reason}");
        // Read-only: the primary is untouched and nothing else appears.
        assert_eq!(fs::read_to_string(&primary).unwrap(), original);
        assert_no_backup_or_temp(dir.path());
    }

    #[test]
    fn test_migration_reload_blocked_for_future_version_without_writing() {
        let dir = temp_dir();
        let original = "version = 42\n";
        let primary = write_file(&dir, "config.toml", original);
        let service = MigrationService::for_primary_path(&primary);
        let error = service.primary_status().unwrap_err();
        assert!(matches!(error, MigrationError::FutureVersion { .. }));
        assert_eq!(fs::read_to_string(&primary).unwrap(), original);
        assert_no_backup_or_temp(dir.path());
    }

    #[test]
    fn test_migration_never_migrates_any_file_but_the_primary() {
        let dir = temp_dir();
        let primary = write_file(&dir, "config.toml", "[bar]\nheight = 48\n");
        let other_files = [
            write_file(&dir, "conf.d/01-a.toml", "[theme]\nfont_family = \"x\"\n"),
            write_file(&dir, "overrides.toml", "[bar]\nwidget_spacing = 6\n"),
        ];
        let before: Vec<Vec<u8>> = other_files.iter().map(|p| fs::read(p).unwrap()).collect();
        MigrationService::for_primary_path(&primary)
            .run(MigrationMode::Apply)
            .unwrap();
        for (path, bytes) in other_files.iter().zip(before) {
            assert_eq!(
                fs::read(path).unwrap(),
                bytes,
                "{} must be untouched",
                path.display()
            );
        }
    }
}
