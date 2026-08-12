use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

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

pub fn discover_sources(config_dir: &Path, primary_path: &Path) -> Vec<DiscoveredSource> {
    let mut sources = Vec::new();

    // 1. Primary file (config.toml)
    if primary_path.exists()
        && fs::metadata(primary_path)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
    {
        sources.push(DiscoveredSource {
            source: ConfigSource::Primary {
                path: primary_path.to_path_buf(),
            },
            path: primary_path.to_path_buf(),
        });
    }

    // 2. conf.d/*.toml fragments
    let conf_d = config_dir.join("conf.d");
    if let Ok(entries) = fs::read_dir(&conf_d) {
        let mut fragment_paths = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("toml")
                && fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false)
            {
                fragment_paths.push(path);
            }
        }
        // Sort by file_name deterministically
        fragment_paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for path in fragment_paths {
            sources.push(DiscoveredSource {
                source: ConfigSource::Fragment { path: path.clone() },
                path,
            });
        }
    }

    // 3. overrides.toml
    let overrides_path = config_dir.join("overrides.toml");
    if overrides_path.exists()
        && fs::metadata(&overrides_path)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
    {
        sources.push(DiscoveredSource {
            source: ConfigSource::Overrides {
                path: overrides_path.clone(),
            },
            path: overrides_path,
        });
    }

    sources
}
