use crate::config::{
    changeset::ConfigChangeSet,
    merge::{initial_merged_document, merge_document, read_source_document},
    provenance::ConfigProvenance,
    source::{ConfigSource, discover_sources, try_discover_sources},
    types::{ConfigDiagnostic, ConfigError, ShellConfig},
    unknown_keys::{UnknownConfigKey, sanitize_document},
    validation::{RecoveryScope, apply_scoped_recovery},
};
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

/// A complete layered candidate: config, provenance, sources, unknown keys.
type CandidateResult = (
    ShellConfig,
    ConfigProvenance,
    Vec<ConfigSource>,
    Vec<UnknownConfigKey>,
);

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigSnapshot {
    pub config: ShellConfig,
    pub provenance: ConfigProvenance,
}

impl Default for ConfigSnapshot {
    fn default() -> Self {
        let (_doc, provenance) = initial_merged_document();
        Self {
            config: ShellConfig::default(),
            provenance,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolutionReport {
    pub diagnostics: Vec<ConfigDiagnostic>,
    pub recovery_scope: Option<RecoveryScope>,
    pub sources_loaded: Vec<ConfigSource>,
    pub unknown_keys: Vec<UnknownConfigKey>,
}

#[derive(Clone, Debug)]
pub struct ConfigResolver {
    pub config_dir: PathBuf,
    pub primary_path: PathBuf,
}

struct LoadedSource {
    source: ConfigSource,
    path: PathBuf,
    doc: DocumentMut,
    text: String,
}

impl ConfigResolver {
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        let config_dir = config_dir.as_ref().to_path_buf();
        let primary_path = config_dir.join("config.toml");
        Self {
            config_dir,
            primary_path,
        }
    }

    pub fn from_primary_path(primary_path: impl AsRef<Path>) -> Self {
        let primary_path = primary_path.as_ref().to_path_buf();
        let config_dir = primary_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Self {
            config_dir,
            primary_path,
        }
    }

    fn load_source_documents(&self) -> Result<Vec<LoadedSource>, ConfigError> {
        try_discover_sources(&self.config_dir, &self.primary_path)?
            .into_iter()
            .map(|disc| {
                let (doc, text) = read_source_document(&disc.path)?;
                if text.trim().is_empty() {
                    return Ok(None);
                }
                Ok(Some(LoadedSource {
                    source: disc.source,
                    path: disc.path,
                    doc,
                    text,
                }))
            })
            .collect::<Result<Vec<_>, ConfigError>>()
            .map(|sources| sources.into_iter().flatten().collect())
    }

    fn merge_loaded_sources(
        loaded: Vec<LoadedSource>,
    ) -> (
        DocumentMut,
        ConfigProvenance,
        Vec<ConfigSource>,
        Vec<UnknownConfigKey>,
    ) {
        let (mut acc_doc, mut provenance) = initial_merged_document();
        let mut sources_loaded = vec![ConfigSource::Defaults];
        let mut unknown_keys = Vec::new();
        for LoadedSource {
            source,
            doc: mut source_doc,
            text,
            ..
        } in loaded
        {
            let mut warnings = sanitize_document(&mut source_doc, &source, &text);
            merge_document(&mut acc_doc, &mut provenance, &source, &source_doc, &text);
            unknown_keys.append(&mut warnings);
            sources_loaded.push(source);
        }
        (acc_doc, provenance, sources_loaded, unknown_keys)
    }

    fn resolve_candidate(&self) -> Result<CandidateResult, ConfigError> {
        let loaded = self.load_source_documents()?;
        let (acc_doc, provenance, sources_loaded, unknown_keys) =
            Self::merge_loaded_sources(loaded);

        let candidate = self.parse_merged(&acc_doc)?;
        Ok((candidate, provenance, sources_loaded, unknown_keys))
    }

    /// Resolve the layered candidate with an in-memory primary document
    /// instead of the file on disk. Used by schema migration to validate a
    /// migrated primary together with the current fragments and overrides
    /// before any filesystem write.
    pub fn resolve_candidate_with_primary(
        &self,
        primary_toml: &str,
    ) -> Result<CandidateResult, ConfigError> {
        let (mut acc_doc, mut provenance) = initial_merged_document();
        let mut sources_loaded = vec![ConfigSource::Defaults];
        let mut unknown_keys = Vec::new();
        let primary_source = ConfigSource::Primary {
            path: self.primary_path.clone(),
        };

        let mut primary_doc: DocumentMut =
            primary_toml
                .parse::<DocumentMut>()
                .map_err(|error| ConfigError::Parse {
                    diagnostic: ConfigDiagnostic {
                        path: self.primary_path.display().to_string(),
                        message: error.to_string(),
                        span: error.span(),
                    },
                })?;
        let mut warnings = sanitize_document(&mut primary_doc, &primary_source, primary_toml);
        merge_document(
            &mut acc_doc,
            &mut provenance,
            &primary_source,
            &primary_doc,
            primary_toml,
        );
        unknown_keys.append(&mut warnings);
        sources_loaded.push(primary_source);

        for disc in discover_sources(&self.config_dir, &self.primary_path) {
            if matches!(disc.source, ConfigSource::Primary { .. }) {
                continue;
            }
            let (mut doc, text) = read_source_document(&disc.path)?;
            let mut warnings = sanitize_document(&mut doc, &disc.source, &text);
            merge_document(&mut acc_doc, &mut provenance, &disc.source, &doc, &text);
            unknown_keys.append(&mut warnings);
            sources_loaded.push(disc.source);
        }

        let candidate = self.parse_merged(&acc_doc)?;
        Ok((candidate, provenance, sources_loaded, unknown_keys))
    }

    /// Resolve the layered candidate with an in-memory overrides document
    /// instead of the file on disk. Used by Settings override writes to
    /// validate an override candidate together with defaults, primary, and fragments
    /// before any filesystem write.
    pub fn resolve_candidate_with_overrides(
        &self,
        overrides_toml: &str,
    ) -> Result<CandidateResult, ConfigError> {
        let (mut acc_doc, mut provenance) = initial_merged_document();
        let mut sources_loaded = vec![ConfigSource::Defaults];
        let mut unknown_keys = Vec::new();
        let overrides_path = self.config_dir.join("overrides.toml");
        let overrides_source = ConfigSource::Overrides {
            path: overrides_path.clone(),
        };

        for disc in discover_sources(&self.config_dir, &self.primary_path) {
            if matches!(disc.source, ConfigSource::Overrides { .. }) {
                continue;
            }
            let (mut doc, text) = read_source_document(&disc.path)?;
            if !matches!(disc.source, ConfigSource::Primary { .. }) && doc.contains_key("version") {
                return Err(ConfigError::Validation {
                    diagnostics: vec![ConfigDiagnostic::new(
                        disc.path.display().to_string(),
                        "'version' belongs only to the primary document",
                    )],
                });
            }
            let mut warnings = sanitize_document(&mut doc, &disc.source, &text);
            merge_document(&mut acc_doc, &mut provenance, &disc.source, &doc, &text);
            unknown_keys.append(&mut warnings);
            sources_loaded.push(disc.source);
        }

        if !overrides_toml.trim().is_empty() {
            let mut overrides_doc: DocumentMut =
                overrides_toml
                    .parse::<DocumentMut>()
                    .map_err(|error| ConfigError::Parse {
                        diagnostic: ConfigDiagnostic {
                            path: overrides_path.display().to_string(),
                            message: error.to_string(),
                            span: error.span(),
                        },
                    })?;

            if overrides_doc.contains_key("version") {
                let ver_item = &overrides_doc["version"];
                let found_ver = ver_item.as_integer().map(|v| v as u64);
                return Err(ConfigError::Validation {
                    diagnostics: vec![ConfigDiagnostic {
                        path: overrides_path.display().to_string(),
                        message: match found_ver {
                            Some(v) => format!(
                                "config {}: 'version' belongs only to the primary document (found version {v})",
                                overrides_path.display()
                            ),
                            None => format!(
                                "config {}: 'version' belongs only to the primary document",
                                overrides_path.display()
                            ),
                        },
                        span: None,
                    }],
                });
            }

            let mut warnings =
                sanitize_document(&mut overrides_doc, &overrides_source, overrides_toml);
            merge_document(
                &mut acc_doc,
                &mut provenance,
                &overrides_source,
                &overrides_doc,
                overrides_toml,
            );
            unknown_keys.append(&mut warnings);
            sources_loaded.push(overrides_source);
        }

        let candidate = self.parse_merged(&acc_doc)?;
        Ok((candidate, provenance, sources_loaded, unknown_keys))
    }

    fn parse_merged(&self, acc_doc: &DocumentMut) -> Result<ShellConfig, ConfigError> {
        let merged_toml = acc_doc.to_string();
        toml::from_str(&merged_toml).map_err(|error| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: self.primary_path.display().to_string(),
                message: error.to_string(),
                span: error.span(),
            },
        })
    }

    pub fn resolve_initial(&self) -> Result<(ConfigSnapshot, ResolutionReport), ConfigError> {
        let (mut candidate, mut provenance, sources_loaded, unknown_keys) =
            self.resolve_candidate()?;
        let default_snapshot = ConfigSnapshot::default();

        match candidate.validate() {
            Ok(()) => Ok((
                ConfigSnapshot {
                    config: candidate,
                    provenance,
                },
                ResolutionReport {
                    diagnostics: Vec::new(),
                    recovery_scope: None,
                    sources_loaded,
                    unknown_keys,
                },
            )),
            Err(ConfigError::Validation { diagnostics }) => {
                let recovery_scope = apply_scoped_recovery(
                    &mut candidate,
                    &mut provenance,
                    &default_snapshot.config,
                    &default_snapshot.provenance,
                    &diagnostics,
                );

                if recovery_scope == RecoveryScope::RejectCandidate {
                    Err(ConfigError::Validation { diagnostics })
                } else {
                    Ok((
                        ConfigSnapshot {
                            config: candidate,
                            provenance,
                        },
                        ResolutionReport {
                            diagnostics,
                            recovery_scope: Some(recovery_scope),
                            sources_loaded,
                            unknown_keys,
                        },
                    ))
                }
            }
            Err(other) => Err(other),
        }
    }

    pub fn resolve_reload(
        &self,
        previous: &ConfigSnapshot,
    ) -> (ConfigSnapshot, ConfigChangeSet, ResolutionReport) {
        let candidate_res = self.resolve_candidate();
        let (mut candidate, mut provenance, sources_loaded, unknown_keys) = match candidate_res {
            Ok(tuple) => tuple,
            Err(err) => {
                let diag = match err {
                    ConfigError::Parse { diagnostic } => diagnostic,
                    _ => ConfigDiagnostic::new(
                        self.primary_path.display().to_string(),
                        err.to_string(),
                    ),
                };
                return (
                    previous.clone(),
                    ConfigChangeSet::default(),
                    ResolutionReport {
                        diagnostics: vec![diag],
                        recovery_scope: Some(RecoveryScope::RejectCandidate),
                        sources_loaded: Vec::new(),
                        unknown_keys: Vec::new(),
                    },
                );
            }
        };

        match candidate.validate() {
            Ok(()) => {
                let snapshot = ConfigSnapshot {
                    config: candidate,
                    provenance,
                };
                let changeset = ConfigChangeSet::compute(&previous.config, &snapshot.config);
                (
                    snapshot,
                    changeset,
                    ResolutionReport {
                        diagnostics: Vec::new(),
                        recovery_scope: None,
                        sources_loaded,
                        unknown_keys,
                    },
                )
            }
            Err(ConfigError::Validation { diagnostics }) => {
                let recovery_scope = apply_scoped_recovery(
                    &mut candidate,
                    &mut provenance,
                    &previous.config,
                    &previous.provenance,
                    &diagnostics,
                );

                if recovery_scope == RecoveryScope::RejectCandidate {
                    (
                        previous.clone(),
                        ConfigChangeSet::default(),
                        ResolutionReport {
                            diagnostics,
                            recovery_scope: Some(RecoveryScope::RejectCandidate),
                            sources_loaded,
                            unknown_keys,
                        },
                    )
                } else {
                    let snapshot = ConfigSnapshot {
                        config: candidate,
                        provenance,
                    };
                    let changeset = ConfigChangeSet::compute(&previous.config, &snapshot.config);
                    (
                        snapshot,
                        changeset,
                        ResolutionReport {
                            diagnostics,
                            recovery_scope: Some(recovery_scope),
                            sources_loaded,
                            unknown_keys,
                        },
                    )
                }
            }
            Err(other) => (
                previous.clone(),
                ConfigChangeSet::default(),
                ResolutionReport {
                    diagnostics: vec![ConfigDiagnostic::new(
                        self.primary_path.display().to_string(),
                        other.to_string(),
                    )],
                    recovery_scope: Some(RecoveryScope::RejectCandidate),
                    sources_loaded,
                    unknown_keys,
                },
            ),
        }
    }

    pub fn load(&self) -> Result<ShellConfig, ConfigError> {
        let (snapshot, _report) = self.resolve_initial()?;
        Ok(snapshot.config)
    }

    pub fn load_or_create(&self) -> Result<ShellConfig, ConfigError> {
        if !self.primary_path.exists() {
            let default_config = ShellConfig::default();
            default_config.save(&self.primary_path)?;
        }
        self.load()
    }

    pub fn inspect_strict(&self) -> Result<ConfigInspection, ConfigInspectionError> {
        let mut loaded = self.load_source_documents().map_err(|err| match err {
            ConfigError::Parse { diagnostic } => ConfigInspectionError::Parse {
                path: PathBuf::from(diagnostic.path),
                message: diagnostic.message,
                span: diagnostic.span,
            },
            ConfigError::Io { path, source } => ConfigInspectionError::ReadFailed {
                path,
                message: source.to_string(),
            },
            other => ConfigInspectionError::ReadFailed {
                path: self.primary_path.clone(),
                message: other.to_string(),
            },
        })?;

        if let Some(primary) = loaded
            .iter()
            .find(|source| matches!(source.source, ConfigSource::Primary { .. }))
        {
            match crate::config::migration::detect_version(
                &primary.doc,
                &primary.path,
                crate::config::migration::LATEST_CONFIG_VERSION,
            ) {
                Err(crate::config::migration::MigrationError::InvalidVersion { path, message }) => {
                    return Err(ConfigInspectionError::InvalidVersion { path, message });
                }
                Err(crate::config::migration::MigrationError::FutureVersion {
                    path,
                    found,
                    latest,
                }) => {
                    return Err(ConfigInspectionError::FutureVersion {
                        path,
                        found,
                        latest,
                    });
                }
                Err(error) => {
                    return Err(ConfigInspectionError::InvalidVersion {
                        path: primary.path.clone(),
                        message: error.to_string(),
                    });
                }
                Ok(None) | Ok(Some(0)) => {
                    return Err(ConfigInspectionError::MigrationRequired {
                        path: primary.path.clone(),
                        from_version: 0,
                    });
                }
                Ok(Some(_)) => {}
            }
        }

        if let Some(source) = loaded.iter().find(|source| {
            !matches!(source.source, ConfigSource::Primary { .. })
                && source.doc.contains_key("version")
        }) {
            let version = source.doc["version"].as_integer().map(|value| value as u64);
            return Err(ConfigInspectionError::InvalidSourceVersion {
                path: source.path.clone(),
                version,
            });
        }

        let (acc_doc, provenance, sources_loaded, unknown_keys) =
            Self::merge_loaded_sources(std::mem::take(&mut loaded));

        let merged_toml = acc_doc.to_string();
        let type_error_source = {
            let (defaults, _) = initial_merged_document();
            acc_doc.iter().find_map(|(key, item)| {
                let default_item = defaults.get(key)?;
                if default_item.is_table() && item.is_value() {
                    provenance
                        .get(key)
                        .and_then(|location| match &location.source {
                            ConfigSource::Defaults => None,
                            ConfigSource::Primary { path }
                            | ConfigSource::Fragment { path }
                            | ConfigSource::Overrides { path } => Some(path.clone()),
                        })
                } else {
                    None
                }
            })
        };
        let candidate: ShellConfig = match toml::from_str(&merged_toml) {
            Ok(candidate) => candidate,
            Err(error) => {
                let message = error.to_string();
                let key = message
                    .split("key `")
                    .nth(1)
                    .and_then(|suffix| suffix.split('`').next());
                let path = key
                    .and_then(|key| provenance.get(key))
                    .and_then(|location| match &location.source {
                        ConfigSource::Defaults => None,
                        ConfigSource::Primary { path }
                        | ConfigSource::Fragment { path }
                        | ConfigSource::Overrides { path } => Some(path.clone()),
                    })
                    .or(type_error_source)
                    .unwrap_or_else(|| self.primary_path.clone());
                return Err(ConfigInspectionError::TypeError {
                    path,
                    message,
                    span: error.span(),
                });
            }
        };

        match candidate.validate() {
            Ok(()) => Ok(ConfigInspection {
                primary_path: self.primary_path.clone(),
                sources_loaded,
                snapshot: ConfigSnapshot {
                    config: candidate,
                    provenance,
                },
                unknown_keys,
            }),
            Err(ConfigError::Validation { diagnostics }) => {
                let source_path = diagnostics
                    .first()
                    .and_then(|diagnostic| provenance.get(&diagnostic.path))
                    .and_then(|location| match &location.source {
                        ConfigSource::Defaults => None,
                        ConfigSource::Primary { path }
                        | ConfigSource::Fragment { path }
                        | ConfigSource::Overrides { path } => Some(path.clone()),
                    })
                    .unwrap_or_else(|| self.primary_path.clone());
                Err(ConfigInspectionError::Validation {
                    diagnostics,
                    source_path,
                })
            }
            Err(other) => Err(ConfigInspectionError::Validation {
                diagnostics: vec![ConfigDiagnostic::new(
                    self.primary_path.display().to_string(),
                    other.to_string(),
                )],
                source_path: self.primary_path.clone(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigInspection {
    pub primary_path: PathBuf,
    pub sources_loaded: Vec<ConfigSource>,
    pub snapshot: ConfigSnapshot,
    pub unknown_keys: Vec<UnknownConfigKey>,
}

#[derive(Debug)]
pub enum ConfigInspectionError {
    ReadFailed {
        path: PathBuf,
        message: String,
    },
    Parse {
        path: PathBuf,
        message: String,
        span: Option<std::ops::Range<usize>>,
    },
    InvalidVersion {
        path: PathBuf,
        message: String,
    },
    FutureVersion {
        path: PathBuf,
        found: u32,
        latest: u32,
    },
    MigrationRequired {
        path: PathBuf,
        from_version: u32,
    },
    InvalidSourceVersion {
        path: PathBuf,
        version: Option<u64>,
    },
    TypeError {
        path: PathBuf,
        message: String,
        span: Option<std::ops::Range<usize>>,
    },
    Validation {
        diagnostics: Vec<ConfigDiagnostic>,
        source_path: PathBuf,
    },
    Serialization {
        message: String,
    },
}

impl ConfigInspectionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ReadFailed { .. } => "config.validation.read_failed",
            Self::Parse { .. } => "config.validation.parse_failed",
            Self::InvalidVersion { .. } => "config.validation.invalid_version",
            Self::FutureVersion { .. } => "config.validation.future_version",
            Self::MigrationRequired { .. } => "config.validation.migration_required",
            Self::InvalidSourceVersion { .. } => "config.validation.invalid_source_version",
            Self::TypeError { .. } => "config.validation.type_error",
            Self::Validation { .. } => "config.validation.semantic_failed",
            Self::Serialization { .. } => "config.validation.serialization_failed",
        }
    }

    pub fn path(&self) -> PathBuf {
        match self {
            Self::ReadFailed { path, .. }
            | Self::Parse { path, .. }
            | Self::InvalidVersion { path, .. }
            | Self::FutureVersion { path, .. }
            | Self::MigrationRequired { path, .. }
            | Self::InvalidSourceVersion { path, .. }
            | Self::TypeError { path, .. } => path.clone(),
            Self::Validation { source_path, .. } => source_path.clone(),
            Self::Serialization { .. } => PathBuf::new(),
        }
    }

    pub fn details(&self) -> serde_json::Value {
        let mut details = serde_json::json!({
            "path": self.path().display().to_string(),
        });
        match self {
            Self::Parse {
                span: Some(span), ..
            }
            | Self::TypeError {
                span: Some(span), ..
            } => {
                details["span"] = serde_json::json!([span.start, span.end]);
            }
            Self::MigrationRequired { from_version, .. } => {
                details["from_version"] = serde_json::json!(from_version);
            }
            Self::FutureVersion { found, latest, .. } => {
                details["found_version"] = serde_json::json!(found);
                details["latest_version"] = serde_json::json!(latest);
            }
            Self::InvalidSourceVersion {
                version: Some(v), ..
            } => {
                details["found_version"] = serde_json::json!(v);
            }
            Self::Validation { diagnostics, .. } => {
                details["diagnostics"] = serde_json::to_value(diagnostics).unwrap_or_default();
            }
            Self::Serialization { message } => {
                details["message"] = serde_json::json!(message);
            }
            _ => {}
        }
        details
    }
}

impl std::fmt::Display for ConfigInspectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadFailed { path, message } => {
                write!(f, "cannot read config {}: {message}", path.display())
            }
            Self::Parse { path, message, .. } => {
                write!(f, "config {}: parse error: {message}", path.display())
            }
            Self::InvalidVersion { path, message } => {
                write!(f, "config {}: invalid version: {message}", path.display())
            }
            Self::FutureVersion {
                path,
                found,
                latest,
            } => write!(
                f,
                "config {}: schema version {found} is newer than latest supported version {latest}",
                path.display()
            ),
            Self::MigrationRequired { path, from_version } => write!(
                f,
                "config {}: schema migration required from version {from_version}; run 'shilpo config migrate'",
                path.display()
            ),
            Self::InvalidSourceVersion { path, version } => {
                write!(
                    f,
                    "config {}: 'version' belongs only to the primary document",
                    path.display()
                )?;
                match version {
                    Some(version) => write!(f, " (found version {version})"),
                    None => Ok(()),
                }
            }
            Self::TypeError { path, message, .. } => {
                write!(f, "config {}: type error: {message}", path.display())
            }
            Self::Validation { diagnostics, .. } => write!(
                f,
                "config validation failed: {}",
                diagnostics
                    .iter()
                    .map(|d| format!("{}: {}", d.path, d.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Self::Serialization { message } => {
                write!(f, "cannot serialize effective config: {message}")
            }
        }
    }
}

impl std::error::Error for ConfigInspectionError {}
