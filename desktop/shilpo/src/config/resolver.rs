use crate::config::{
    changeset::ConfigChangeSet,
    merge::{initial_merged_document, merge_source},
    provenance::ConfigProvenance,
    source::{ConfigSource, discover_sources},
    types::{ConfigDiagnostic, ConfigError, ShellConfig},
    validation::{RecoveryScope, apply_scoped_recovery},
};
use std::path::{Path, PathBuf};

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

    fn resolve_candidate(
        &self,
    ) -> Result<(ShellConfig, ConfigProvenance, Vec<ConfigSource>), ConfigError> {
        let discovered = discover_sources(&self.config_dir, &self.primary_path);
        let (mut acc_doc, mut provenance) = initial_merged_document();
        let mut sources_loaded = vec![ConfigSource::Defaults];

        for disc in discovered {
            merge_source(&mut acc_doc, &mut provenance, &disc.source, &disc.path)?;
            sources_loaded.push(disc.source);
        }

        let merged_toml = acc_doc.to_string();
        let candidate: ShellConfig =
            toml::from_str(&merged_toml).map_err(|error| ConfigError::Parse {
                diagnostic: ConfigDiagnostic {
                    path: self.primary_path.display().to_string(),
                    message: error.to_string(),
                    span: error.span(),
                },
            })?;

        Ok((candidate, provenance, sources_loaded))
    }

    pub fn resolve_initial(&self) -> Result<(ConfigSnapshot, ResolutionReport), ConfigError> {
        let (mut candidate, mut provenance, sources_loaded) = self.resolve_candidate()?;
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
        let (mut candidate, mut provenance, sources_loaded) = match candidate_res {
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
