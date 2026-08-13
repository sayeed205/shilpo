use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConfigSource {
    Defaults,
    Primary { path: PathBuf },
    Fragment { path: PathBuf },
    Overrides { path: PathBuf },
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Defaults => write!(f, "defaults"),
            Self::Primary { path } => write!(f, "{}", path.display()),
            Self::Fragment { path } => write!(f, "{}", path.display()),
            Self::Overrides { path } => write!(f, "{}", path.display()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    pub source: ConfigSource,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl SourceLocation {
    pub fn defaults() -> Self {
        Self {
            source: ConfigSource::Defaults,
            line: None,
            column: None,
        }
    }
}

pub struct DiscoveredSource {
    pub source: ConfigSource,
    pub path: PathBuf,
}

/// Discover all config sources while preserving filesystem errors for callers
/// that need strict, user-facing diagnostics. Empty files are omitted; callers
/// that need to distinguish whitespace-only files can inspect the source text.
pub fn try_discover_sources(
    config_dir: &Path,
    primary_path: &Path,
) -> Result<Vec<DiscoveredSource>, crate::config::types::ConfigError> {
    let mut sources = Vec::new();

    let metadata = match fs::metadata(primary_path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(crate::config::types::ConfigError::Io {
                path: primary_path.to_path_buf(),
                source,
            });
        }
    };
    if let Some(metadata) = metadata
        && metadata.is_file()
        && metadata.len() > 0
    {
        sources.push(DiscoveredSource {
            source: ConfigSource::Primary {
                path: primary_path.to_path_buf(),
            },
            path: primary_path.to_path_buf(),
        });
    }

    let conf_d = config_dir.join("conf.d");
    let entries = match fs::read_dir(&conf_d) {
        Ok(entries) => Some(entries),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(crate::config::types::ConfigError::Io {
                path: conf_d,
                source,
            });
        }
    };
    if let Some(entries) = entries {
        let mut fragment_paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| crate::config::types::ConfigError::Io {
                path: config_dir.join("conf.d"),
                source,
            })?;
            let path = entry.path();
            let metadata =
                fs::metadata(&path).map_err(|source| crate::config::types::ConfigError::Io {
                    path: path.clone(),
                    source,
                })?;
            if metadata.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("toml")
                && metadata.len() > 0
            {
                fragment_paths.push(path);
            }
        }
        fragment_paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
        sources.extend(fragment_paths.into_iter().map(|path| DiscoveredSource {
            source: ConfigSource::Fragment { path: path.clone() },
            path,
        }));
    }

    let overrides_path = config_dir.join("overrides.toml");
    let metadata = match fs::metadata(&overrides_path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(crate::config::types::ConfigError::Io {
                path: overrides_path,
                source,
            });
        }
    };
    if let Some(metadata) = metadata
        && metadata.is_file()
        && metadata.len() > 0
    {
        sources.push(DiscoveredSource {
            source: ConfigSource::Overrides {
                path: overrides_path.clone(),
            },
            path: overrides_path,
        });
    }

    Ok(sources)
}

pub fn discover_sources(config_dir: &Path, primary_path: &Path) -> Vec<DiscoveredSource> {
    try_discover_sources(config_dir, primary_path).unwrap_or_default()
}
