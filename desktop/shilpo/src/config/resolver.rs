use crate::config::{
    changeset::ConfigChangeSet,
    merge::{initial_merged_document, merge_document, read_source_document},
    provenance::ConfigProvenance,
    source::{ConfigSource, discover_sources},
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

    fn resolve_candidate(&self) -> Result<CandidateResult, ConfigError> {
        let (mut acc_doc, mut provenance) = initial_merged_document();
        let mut sources_loaded = vec![ConfigSource::Defaults];
        let mut unknown_keys = Vec::new();

        for disc in discover_sources(&self.config_dir, &self.primary_path) {
            // Parse, scan for unknown keys (warnings only), and merge the
            // sanitized in-memory document. User files are never rewritten.
            let (mut doc, text) = read_source_document(&disc.path)?;
            let mut warnings = sanitize_document(&mut doc, &disc.source, &text);
            merge_document(&mut acc_doc, &mut provenance, &disc.source, &doc, &text);
            unknown_keys.append(&mut warnings);
            sources_loaded.push(disc.source);
        }

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
}
