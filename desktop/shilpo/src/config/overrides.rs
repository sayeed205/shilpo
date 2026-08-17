//! Reusable Settings-facing override write service (`overrides.toml`).
//!
//! Provides transactional comment-preserving edits (`Set` and `Remove` batches)
//! for `overrides.toml` using [`toml_edit::DocumentMut`].

use std::{
    fmt, fs, io,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::config::{
    resolver::ConfigResolver,
    types::{ConfigDiagnostic, ConfigError},
    unknown_keys::UnknownConfigKey,
};

/// A single edit inside a transactional override batch.
#[derive(Debug, Clone)]
pub enum OverrideEdit {
    /// Set a leaf value at the segment path.
    Set { path: Vec<String>, value: Value },
    /// Remove a leaf at the segment path.
    Remove { path: Vec<String> },
}

impl PartialEq for OverrideEdit {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Set {
                    path: p1,
                    value: v1,
                },
                Self::Set {
                    path: p2,
                    value: v2,
                },
            ) => p1 == p2 && v1.to_string() == v2.to_string(),
            (Self::Remove { path: p1 }, Self::Remove { path: p2 }) => p1 == p2,
            _ => false,
        }
    }
}

impl OverrideEdit {
    pub fn set(path: impl IntoIterator<Item = impl Into<String>>, value: impl Into<Value>) -> Self {
        Self::Set {
            path: path.into_iter().map(Into::into).collect(),
            value: value.into(),
        }
    }

    pub fn remove(path: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::Remove {
            path: path.into_iter().map(Into::into).collect(),
        }
    }
}

/// The result of an override transaction run.
#[derive(Debug, Clone, PartialEq)]
pub struct OverrideOutcome {
    pub path: PathBuf,
    pub changed: bool,
    pub warnings: Vec<UnknownConfigKey>,
}

/// Structured error for override operations.
#[derive(Debug)]
pub enum OverrideError {
    InvalidPath {
        path: Vec<String>,
        reason: String,
    },
    VersionForbidden {
        path: Vec<String>,
    },
    TraversalConflict {
        path: Vec<String>,
        segment: String,
        reason: String,
    },
    ReadFailed {
        path: PathBuf,
        source: io::Error,
    },
    ParseFailed {
        path: PathBuf,
        message: String,
        span: Option<std::ops::Range<usize>>,
    },
    CandidateValidationFailed {
        diagnostics: Vec<ConfigDiagnostic>,
    },
    ConcurrentModification {
        path: PathBuf,
    },
    WriteFailed {
        path: PathBuf,
        source: io::Error,
    },
    TempCleanupFailed {
        path: PathBuf,
        source: io::Error,
    },
    DurabilityFailed {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for OverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath { path, reason } => {
                write!(f, "invalid override path {:?}: {reason}", path)
            }
            Self::VersionForbidden { path } => {
                write!(
                    f,
                    "override path {:?}: 'version' belongs only to primary config.toml",
                    path
                )
            }
            Self::TraversalConflict {
                path,
                segment,
                reason,
            } => {
                write!(
                    f,
                    "traversal conflict at segment '{segment}' for path {:?}: {reason}",
                    path
                )
            }
            Self::ReadFailed { path, source } => {
                write!(f, "cannot read overrides {}: {source}", path.display())
            }
            Self::ParseFailed { path, message, .. } => {
                write!(f, "overrides {}: {message}", path.display())
            }
            Self::CandidateValidationFailed { diagnostics } => {
                write!(
                    f,
                    "override candidate failed layered validation: {}",
                    diagnostics
                        .iter()
                        .map(|d| format!("{}: {}", d.path, d.message))
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            }
            Self::ConcurrentModification { path } => {
                write!(
                    f,
                    "overrides {} changed concurrently during transaction",
                    path.display()
                )
            }
            Self::WriteFailed { path, source } => {
                write!(f, "cannot write overrides {}: {source}", path.display())
            }
            Self::TempCleanupFailed { path, source } => {
                write!(
                    f,
                    "cannot remove temporary override file {}: {source}",
                    path.display()
                )
            }
            Self::DurabilityFailed { path, source } => {
                write!(
                    f,
                    "cannot sync directory after overrides commit {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for OverrideError {}

/// File operations trait for atomic commit and test injection.
pub trait FileOps: Send + Sync {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
    fn sync_all(&mut self) -> io::Result<()>;
}

/// Filesystem seam for `ConfigOverrideService`.
pub trait OverrideFs: Send + Sync {
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn read_metadata(&self, path: &Path) -> io::Result<fs::Metadata>;
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn FileOps>>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()>;
    fn set_user_only_permissions(&self, path: &Path) -> io::Result<()>;
    fn sync_directory(&self, path: &Path) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
}

/// Production implementation of [`OverrideFs`].
#[derive(Clone, Debug, Default)]
pub struct StdOverrideFs;

impl OverrideFs for StdOverrideFs {
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

    fn set_user_only_permissions(&self, path: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(())
        }
    }

    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        fs::File::open(path)?.sync_all()
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }
}

struct StdFile(fs::File);

impl FileOps for StdFile {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write_all(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }

    fn sync_all(&mut self) -> io::Result<()> {
        self.0.sync_all()
    }
}

/// Service that performs transactional, comment-preserving edits against `overrides.toml`.
#[derive(Clone)]
pub struct ConfigOverrideService {
    pub config_dir: PathBuf,
    pub primary_path: PathBuf,
    pub overrides_path: PathBuf,
    fs: Arc<dyn OverrideFs>,
}

impl ConfigOverrideService {
    pub fn for_primary_path(primary_path: impl AsRef<Path>) -> Self {
        Self::with_fs(primary_path, Arc::new(StdOverrideFs))
    }

    pub fn with_fs(primary_path: impl AsRef<Path>, fs: Arc<dyn OverrideFs>) -> Self {
        let primary_path = primary_path.as_ref().to_path_buf();
        let config_dir = primary_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let overrides_path = config_dir.join("overrides.toml");
        Self {
            config_dir,
            primary_path,
            overrides_path,
            fs,
        }
    }

    /// Apply a batch of `Set` / `Remove` edits to `overrides.toml`.
    pub fn apply_batch(&self, edits: &[OverrideEdit]) -> Result<OverrideOutcome, OverrideError> {
        let (original_text, original_metadata) = match self.fs.read_to_string(&self.overrides_path)
        {
            Ok(text) => {
                let metadata = self
                    .fs
                    .read_metadata(&self.overrides_path)
                    .map_err(|source| OverrideError::ReadFailed {
                        path: self.overrides_path.clone(),
                        source,
                    })?;
                (text, Some(metadata))
            }
            Err(source) if source.kind() == io::ErrorKind::NotFound => (String::new(), None),
            Err(source) => {
                return Err(OverrideError::ReadFailed {
                    path: self.overrides_path.clone(),
                    source,
                });
            }
        };

        let mut doc: DocumentMut = if original_text.trim().is_empty() {
            DocumentMut::new()
        } else {
            original_text
                .parse::<DocumentMut>()
                .map_err(|error| OverrideError::ParseFailed {
                    path: self.overrides_path.clone(),
                    message: error.to_string(),
                    span: error.span(),
                })?
        };

        apply_edits(&mut doc, edits)?;

        let new_text = doc.to_string();

        // 1. No-op check: if rendered TOML equals original text, zero writes!
        if new_text == original_text || documents_semantically_equal(&original_text, &new_text) {
            return Ok(OverrideOutcome {
                path: self.overrides_path.clone(),
                changed: false,
                warnings: Vec::new(),
            });
        }

        // 2. Layered validation of the candidate override
        let resolver = ConfigResolver::from_primary_path(&self.primary_path);
        let (candidate, _provenance, _sources, unknown_keys) = resolver
            .resolve_candidate_with_overrides(&new_text)
            .map_err(|err| match err {
                ConfigError::Validation { diagnostics } => {
                    OverrideError::CandidateValidationFailed { diagnostics }
                }
                other => OverrideError::CandidateValidationFailed {
                    diagnostics: vec![ConfigDiagnostic::new(
                        self.overrides_path.display().to_string(),
                        other.to_string(),
                    )],
                },
            })?;

        candidate.validate().map_err(|err| match err {
            ConfigError::Validation { diagnostics } => {
                OverrideError::CandidateValidationFailed { diagnostics }
            }
            other => OverrideError::CandidateValidationFailed {
                diagnostics: vec![ConfigDiagnostic::new(
                    self.overrides_path.display().to_string(),
                    other.to_string(),
                )],
            },
        })?;

        // 4. Ensure directory exists (without creating config.toml or conf.d)
        if let Err(source) = self.fs.create_dir_all(&self.config_dir) {
            return Err(OverrideError::WriteFailed {
                path: self.config_dir.clone(),
                source,
            });
        }

        // 5. Create temporary sibling file
        // Format: overrides.tmp.<pid>.<uuid> (extension is .tmp, NOT .toml)
        let temp_filename = format!(
            "overrides.tmp.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        );
        let temp_path = self.config_dir.join(temp_filename);

        let mut temp_file = match self.fs.create_new(&temp_path) {
            Ok(file) => file,
            Err(source) => {
                return Err(OverrideError::WriteFailed {
                    path: temp_path,
                    source,
                });
            }
        };

        // Preserve permissions
        if let Some(ref meta) = original_metadata {
            if let Err(source) = self.fs.set_permissions(&temp_path, meta.permissions()) {
                return Err(self.cleanup_after_failure(
                    &temp_path,
                    OverrideError::WriteFailed {
                        path: temp_path.clone(),
                        source,
                    },
                ));
            }
        } else if let Err(source) = self.fs.set_user_only_permissions(&temp_path) {
            return Err(self.cleanup_after_failure(
                &temp_path,
                OverrideError::WriteFailed {
                    path: temp_path.clone(),
                    source,
                },
            ));
        }

        // Write bytes, flush, sync
        if let Err(source) = temp_file.write_all(new_text.as_bytes()) {
            return Err(self.cleanup_after_failure(
                &temp_path,
                OverrideError::WriteFailed {
                    path: self.overrides_path.clone(),
                    source,
                },
            ));
        }

        if let Err(source) = temp_file.flush() {
            return Err(self.cleanup_after_failure(
                &temp_path,
                OverrideError::WriteFailed {
                    path: self.overrides_path.clone(),
                    source,
                },
            ));
        }

        if let Err(source) = temp_file.sync_all() {
            return Err(self.cleanup_after_failure(
                &temp_path,
                OverrideError::WriteFailed {
                    path: self.overrides_path.clone(),
                    source,
                },
            ));
        }
        drop(temp_file);

        // Re-verify on-disk overrides text and metadata before commit (concurrent modification check)
        if !self.overrides_unchanged(&original_text, &original_metadata) {
            return Err(self.cleanup_after_failure(
                &temp_path,
                OverrideError::ConcurrentModification {
                    path: self.overrides_path.clone(),
                },
            ));
        }

        // Atomic rename
        if let Err(source) = self.fs.rename(&temp_path, &self.overrides_path) {
            return Err(self.cleanup_after_failure(
                &temp_path,
                OverrideError::WriteFailed {
                    path: self.overrides_path.clone(),
                    source,
                },
            ));
        }

        // Parent directory sync
        if let Err(source) = self.fs.sync_directory(&self.config_dir) {
            return Err(OverrideError::DurabilityFailed {
                path: self.config_dir.clone(),
                source,
            });
        }

        Ok(OverrideOutcome {
            path: self.overrides_path.clone(),
            changed: true,
            warnings: unknown_keys,
        })
    }

    fn overrides_unchanged(
        &self,
        original_text: &str,
        original_metadata: &Option<fs::Metadata>,
    ) -> bool {
        let current_text = match self.fs.read_to_string(&self.overrides_path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(_) => return false,
        };

        if current_text != original_text {
            return false;
        }

        if let Some(orig_meta) = original_metadata {
            let Ok(curr_meta) = self.fs.read_metadata(&self.overrides_path) else {
                return false;
            };
            if !metadata_matches(orig_meta, &curr_meta) {
                return false;
            }
        } else if self.fs.read_metadata(&self.overrides_path).is_ok() {
            return false;
        }
        true
    }

    fn cleanup_after_failure(&self, temp_path: &Path, failure: OverrideError) -> OverrideError {
        match self.fs.remove_file(temp_path) {
            Ok(()) => failure,
            Err(source) => OverrideError::TempCleanupFailed {
                path: temp_path.to_path_buf(),
                source,
            },
        }
    }
}

fn documents_semantically_equal(original: &str, candidate: &str) -> bool {
    if original.trim().is_empty() && candidate.trim().is_empty() {
        return true;
    }
    let Ok(original) = original.parse::<toml::Value>() else {
        return false;
    };
    let Ok(candidate) = candidate.parse::<toml::Value>() else {
        return false;
    };
    original == candidate
}

fn metadata_matches(expected: &fs::Metadata, actual: &fs::Metadata) -> bool {
    if expected.len() != actual.len()
        || expected.permissions().readonly() != actual.permissions().readonly()
        || expected.modified().ok() != actual.modified().ok()
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        expected.dev() == actual.dev()
            && expected.ino() == actual.ino()
            && expected.mode() == actual.mode()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn apply_edits(doc: &mut DocumentMut, edits: &[OverrideEdit]) -> Result<(), OverrideError> {
    for edit in edits {
        match edit {
            OverrideEdit::Set { path, value } => {
                validate_override_path(path)?;
                set_leaf(doc, path, value)?;
            }
            OverrideEdit::Remove { path } => {
                validate_override_path(path)?;
                remove_leaf(doc, path)?;
            }
        }
    }
    Ok(())
}

fn validate_override_path(path: &[String]) -> Result<(), OverrideError> {
    if path.is_empty() {
        return Err(OverrideError::InvalidPath {
            path: path.to_vec(),
            reason: "path cannot be empty".to_string(),
        });
    }
    for seg in path {
        if seg.is_empty() {
            return Err(OverrideError::InvalidPath {
                path: path.to_vec(),
                reason: "path segment cannot be empty".to_string(),
            });
        }
    }
    if path.first().map(|s| s.as_str()) == Some("version") {
        return Err(OverrideError::VersionForbidden {
            path: path.to_vec(),
        });
    }
    Ok(())
}

fn set_leaf(doc: &mut DocumentMut, path: &[String], new_val: &Value) -> Result<(), OverrideError> {
    let len = path.len();
    let mut current_table = doc.as_table_mut();

    for (idx, seg) in path.iter().enumerate() {
        if idx == len - 1 {
            if let Some(existing_item) = current_table.get_mut(seg) {
                match existing_item {
                    Item::Value(existing_val) => {
                        let mut val = new_val.clone();
                        *val.decor_mut() = existing_val.decor().clone();
                        *existing_item = Item::Value(val);
                    }
                    Item::Table(_) | Item::ArrayOfTables(_) => {
                        return Err(OverrideError::TraversalConflict {
                            path: path.to_vec(),
                            segment: seg.clone(),
                            reason: "cannot replace an existing table with a leaf value"
                                .to_string(),
                        });
                    }
                    Item::None => *existing_item = Item::Value(new_val.clone()),
                }
            } else {
                current_table.insert(seg, Item::Value(new_val.clone()));
            }
        } else {
            if !current_table.contains_key(seg) {
                current_table.insert(seg, Item::Table(Table::new()));
            }
            let item = current_table.get_mut(seg).unwrap();
            match item {
                Item::Table(t) => {
                    current_table = t;
                }
                Item::Value(Value::InlineTable(_)) => {
                    return Err(OverrideError::TraversalConflict {
                        path: path.to_vec(),
                        segment: seg.clone(),
                        reason: "cannot traverse through existing inline table".to_string(),
                    });
                }
                Item::Value(_) | Item::ArrayOfTables(_) => {
                    return Err(OverrideError::TraversalConflict {
                        path: path.to_vec(),
                        segment: seg.clone(),
                        reason: format!(
                            "cannot traverse through scalar or array at segment '{seg}'"
                        ),
                    });
                }
                Item::None => unreachable!(),
            }
        }
    }
    Ok(())
}

fn remove_leaf(doc: &mut DocumentMut, path: &[String]) -> Result<(), OverrideError> {
    let len = path.len();
    let mut current_table = doc.as_table_mut();

    for (idx, seg) in path.iter().enumerate() {
        if idx == len - 1 {
            current_table.remove(seg);
        } else {
            let Some(item) = current_table.get_mut(seg) else {
                return Ok(());
            };
            match item {
                Item::Table(t) => {
                    current_table = t;
                }
                Item::Value(Value::InlineTable(_)) => {
                    return Err(OverrideError::TraversalConflict {
                        path: path.to_vec(),
                        segment: seg.clone(),
                        reason: "cannot traverse through existing inline table".to_string(),
                    });
                }
                Item::Value(_) | Item::ArrayOfTables(_) => {
                    return Err(OverrideError::TraversalConflict {
                        path: path.to_vec(),
                        segment: seg.clone(),
                        reason: format!(
                            "cannot traverse through scalar or array at segment '{seg}'"
                        ),
                    });
                }
                Item::None => return Ok(()),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_formatting_and_comments_preserved_on_leaf_replacement() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");

        let content = r#"# Top-level header comment

[theme] # inline theme comment
font_family = "Inter" # inline font comment
corner_radius_scale = 1.0 # radius comment

# Suffix comment
"#;
        std::fs::write(&overrides, content).unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);
        let outcome = service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap();

        assert!(outcome.changed);
        let updated = std::fs::read_to_string(&overrides).unwrap();
        assert!(updated.contains("# Top-level header comment"));
        assert!(updated.contains("# inline theme comment"));
        assert!(updated.contains("font_family = \"Roboto\" # inline font comment"));
        assert!(updated.contains("# inline font comment"));
        assert!(updated.contains("# Suffix comment"));
    }

    #[test]
    fn test_leaf_decoration_survives_replacement() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");

        std::fs::write(&overrides, "[bar]\nheight = 48 # Keep this comment\n").unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);
        service
            .apply_batch(&[OverrideEdit::set(["bar", "height"], 64)])
            .unwrap();

        let updated = std::fs::read_to_string(&overrides).unwrap();
        assert!(updated.contains("height = 64 # Keep this comment"));
    }

    #[test]
    fn test_nested_leaf_insertion_creates_tables_and_preserves_content() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");

        std::fs::write(&overrides, "[theme]\nfont_family = \"Inter\"\n").unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);
        service
            .apply_batch(&[OverrideEdit::set(["bar", "height"], 56)])
            .unwrap();

        let updated = std::fs::read_to_string(&overrides).unwrap();
        assert!(updated.contains("font_family = \"Inter\""));
        assert!(updated.contains("[bar]"));
        assert!(updated.contains("height = 56"));
    }

    #[test]
    fn test_dynamic_key_containing_dots_treated_as_single_quoted_key() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");

        let service = ConfigOverrideService::for_primary_path(&primary);
        let val: Value = "{ foo = \"bar\" }".parse().unwrap();
        service
            .apply_batch(&[OverrideEdit::set(
                ["extensions", "settings", "org.example.clock"],
                val,
            )])
            .unwrap();

        let updated = std::fs::read_to_string(&overrides).unwrap();
        assert!(updated.contains("\"org.example.clock\""));
        assert!(!updated.contains("[extensions.settings.org.example.clock]"));
    }

    #[test]
    fn test_removing_leaf_preserves_siblings_and_empty_table() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");

        std::fs::write(
            &overrides,
            "[theme] # theme section\nfont_family = \"Inter\"\naccent = \"#123456\"\n",
        )
        .unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);
        service
            .apply_batch(&[OverrideEdit::remove(["theme", "accent"])])
            .unwrap();

        let updated = std::fs::read_to_string(&overrides).unwrap();
        assert!(updated.contains("font_family = \"Inter\""));
        assert!(!updated.contains("accent"));
        assert!(updated.contains("[theme] # theme section"));

        // Remove font_family as well; empty table & comment remain
        service
            .apply_batch(&[OverrideEdit::remove(["theme", "font_family"])])
            .unwrap();
        let updated2 = std::fs::read_to_string(&overrides).unwrap();
        assert!(updated2.contains("[theme] # theme section"));
    }

    #[test]
    fn test_noop_batch_returns_changed_false_and_zero_writes() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");

        std::fs::write(&overrides, "[theme]\nfont_family = \"Inter\"\n").unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);

        // Same value set
        let outcome1 = service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Inter")])
            .unwrap();
        assert!(!outcome1.changed);

        // Remove absent leaf
        let outcome2 = service
            .apply_batch(&[OverrideEdit::remove(["theme", "absent_key"])])
            .unwrap();
        assert!(!outcome2.changed);
    }

    #[test]
    fn test_missing_and_whitespace_override_file() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);
        let outcome = service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap();

        assert!(outcome.changed);
        assert!(service.overrides_path.exists());

        let whitespace = "  \n\n";
        std::fs::write(&service.overrides_path, whitespace).unwrap();
        let outcome = service
            .apply_batch(&[OverrideEdit::remove(["theme", "font_family"])])
            .unwrap();
        assert!(!outcome.changed);
        assert_eq!(
            std::fs::read_to_string(&service.overrides_path).unwrap(),
            whitespace
        );
    }

    #[test]
    fn test_invalid_toml_blocks_with_zero_writes() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");

        let invalid_content = "invalid toml {{{ syntax\n";
        std::fs::write(&overrides, invalid_content).unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);
        let err = service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap_err();

        assert!(matches!(err, OverrideError::ParseFailed { .. }));
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            invalid_content
        );
    }

    #[test]
    fn test_empty_paths_traversal_conflicts_and_version_rejected() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");
        std::fs::write(&overrides, "[theme]\nfont_family = \"Inter\"\n").unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);

        // Empty path
        assert!(matches!(
            service.apply_batch(&[OverrideEdit::set(Vec::<String>::new(), "x")]),
            Err(OverrideError::InvalidPath { .. })
        ));

        // Empty segment
        assert!(matches!(
            service.apply_batch(&[OverrideEdit::set(["theme", ""], "x")]),
            Err(OverrideError::InvalidPath { .. })
        ));

        // Top-level version mutation
        assert!(matches!(
            service.apply_batch(&[OverrideEdit::set(["version"], 1)]),
            Err(OverrideError::VersionForbidden { .. })
        ));

        // Traversal conflict (font_family is scalar)
        assert!(matches!(
            service.apply_batch(&[OverrideEdit::set(["theme", "font_family", "sub"], "x")]),
            Err(OverrideError::TraversalConflict { .. })
        ));

        // A table at the addressed leaf is not replaceable by a scalar.
        assert!(matches!(
            service.apply_batch(&[OverrideEdit::set(["theme"], "x")]),
            Err(OverrideError::TraversalConflict { .. })
        ));
    }

    #[test]
    fn test_full_layered_validation_blocks_invalid_candidate() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");

        let service = ConfigOverrideService::for_primary_path(&primary);
        // bar.height = 999 violates validation max (64)
        let err = service
            .apply_batch(&[OverrideEdit::set(["bar", "height"], 999)])
            .unwrap_err();

        assert!(matches!(
            err,
            OverrideError::CandidateValidationFailed { .. }
        ));
        assert!(!overrides.exists());

        let conf_d = temp.path().join("conf.d");
        std::fs::create_dir_all(&conf_d).unwrap();
        std::fs::write(conf_d.join("01-version.toml"), "version = 1\n").unwrap();
        let err = service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap_err();
        assert!(matches!(
            err,
            OverrideError::CandidateValidationFailed { .. }
        ));
    }

    #[test]
    fn test_unknown_keys_preserved_and_returned_as_warnings() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");

        std::fs::write(&overrides, "unknown_key = 123\n").unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);
        let outcome = service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap();

        assert!(outcome.changed);
        let updated = std::fs::read_to_string(&overrides).unwrap();
        assert!(updated.contains("unknown_key = 123"));
        assert!(!outcome.warnings.is_empty());
    }

    #[test]
    fn test_primary_and_fragment_files_remain_untouched() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        let primary_content = "version = 1\n[theme]\nfont_family = \"Inter\"\n";
        std::fs::write(&primary, primary_content).unwrap();

        let conf_d = temp.path().join("conf.d");
        std::fs::create_dir_all(&conf_d).unwrap();
        let frag = conf_d.join("01-frag.toml");
        let frag_content = "[bar]\nheight = 40\n";
        std::fs::write(&frag, frag_content).unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);
        service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap();

        assert_eq!(std::fs::read_to_string(&primary).unwrap(), primary_content);
        assert_eq!(std::fs::read_to_string(&frag).unwrap(), frag_content);
    }

    #[test]
    fn test_unix_permissions_preserved_or_set_to_user_only() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();

        let service = ConfigOverrideService::for_primary_path(&primary);
        service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&service.overrides_path).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn test_concurrent_external_modification_detected() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");
        std::fs::write(&overrides, "[theme]\nfont_family = \"Inter\"\n").unwrap();

        // Custom FS that mutates the file between read and write
        struct ConcurrencyFs {
            base: StdOverrideFs,
            target: PathBuf,
        }
        impl OverrideFs for ConcurrencyFs {
            fn read_to_string(&self, path: &Path) -> io::Result<String> {
                self.base.read_to_string(path)
            }
            fn read_metadata(&self, path: &Path) -> io::Result<fs::Metadata> {
                self.base.read_metadata(path)
            }
            fn create_new(&self, path: &Path) -> io::Result<Box<dyn FileOps>> {
                // Simulate concurrent external modification before create_new
                std::fs::write(&self.target, "[theme]\nfont_family = \"External\"\n").unwrap();
                self.base.create_new(path)
            }
            fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
                self.base.rename(from, to)
            }
            fn remove_file(&self, path: &Path) -> io::Result<()> {
                self.base.remove_file(path)
            }
            fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()> {
                self.base.set_permissions(path, permissions)
            }
            fn set_user_only_permissions(&self, path: &Path) -> io::Result<()> {
                self.base.set_user_only_permissions(path)
            }
            fn sync_directory(&self, path: &Path) -> io::Result<()> {
                self.base.sync_directory(path)
            }
            fn create_dir_all(&self, path: &Path) -> io::Result<()> {
                self.base.create_dir_all(path)
            }
        }

        let fs = Arc::new(ConcurrencyFs {
            base: StdOverrideFs,
            target: overrides.clone(),
        });
        let service = ConfigOverrideService::with_fs(&primary, fs);

        let err = service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap_err();

        assert!(matches!(err, OverrideError::ConcurrentModification { .. }));
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "[theme]\nfont_family = \"External\"\n"
        );
    }

    #[test]
    fn test_injected_pre_rename_failure_cleans_temp() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();
        let overrides = temp.path().join("overrides.toml");
        std::fs::write(&overrides, "[theme]\nfont_family = \"Inter\"\n").unwrap();

        struct FailingRenameFs {
            base: StdOverrideFs,
        }
        impl OverrideFs for FailingRenameFs {
            fn read_to_string(&self, path: &Path) -> io::Result<String> {
                self.base.read_to_string(path)
            }
            fn read_metadata(&self, path: &Path) -> io::Result<fs::Metadata> {
                self.base.read_metadata(path)
            }
            fn create_new(&self, path: &Path) -> io::Result<Box<dyn FileOps>> {
                self.base.create_new(path)
            }
            fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected rename failure",
                ))
            }
            fn remove_file(&self, path: &Path) -> io::Result<()> {
                self.base.remove_file(path)
            }
            fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()> {
                self.base.set_permissions(path, permissions)
            }
            fn set_user_only_permissions(&self, path: &Path) -> io::Result<()> {
                self.base.set_user_only_permissions(path)
            }
            fn sync_directory(&self, path: &Path) -> io::Result<()> {
                self.base.sync_directory(path)
            }
            fn create_dir_all(&self, path: &Path) -> io::Result<()> {
                self.base.create_dir_all(path)
            }
        }

        let service = ConfigOverrideService::with_fs(
            &primary,
            Arc::new(FailingRenameFs {
                base: StdOverrideFs,
            }),
        );
        let err = service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap_err();

        assert!(matches!(err, OverrideError::WriteFailed { .. }));
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "[theme]\nfont_family = \"Inter\"\n"
        );

        // Verify no leftover temp files
        let entries: Vec<_> = std::fs::read_dir(temp.path()).unwrap().flatten().collect();
        assert_eq!(entries.len(), 2); // config.toml and overrides.toml
    }

    #[test]
    fn test_directory_sync_failure_distinguishable_from_write_failure() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("config.toml");
        std::fs::write(&primary, "version = 1\n").unwrap();

        struct FailingDirSyncFs {
            base: StdOverrideFs,
        }
        impl OverrideFs for FailingDirSyncFs {
            fn read_to_string(&self, path: &Path) -> io::Result<String> {
                self.base.read_to_string(path)
            }
            fn read_metadata(&self, path: &Path) -> io::Result<fs::Metadata> {
                self.base.read_metadata(path)
            }
            fn create_new(&self, path: &Path) -> io::Result<Box<dyn FileOps>> {
                self.base.create_new(path)
            }
            fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
                self.base.rename(from, to)
            }
            fn remove_file(&self, path: &Path) -> io::Result<()> {
                self.base.remove_file(path)
            }
            fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()> {
                self.base.set_permissions(path, permissions)
            }
            fn set_user_only_permissions(&self, path: &Path) -> io::Result<()> {
                self.base.set_user_only_permissions(path)
            }
            fn sync_directory(&self, _path: &Path) -> io::Result<()> {
                Err(io::Error::other("injected dir sync failure"))
            }
            fn create_dir_all(&self, path: &Path) -> io::Result<()> {
                self.base.create_dir_all(path)
            }
        }

        let service = ConfigOverrideService::with_fs(
            &primary,
            Arc::new(FailingDirSyncFs {
                base: StdOverrideFs,
            }),
        );
        let err = service
            .apply_batch(&[OverrideEdit::set(["theme", "font_family"], "Roboto")])
            .unwrap_err();

        assert!(matches!(err, OverrideError::DurabilityFailed { .. }));
        // The rename succeeded before dir sync failed
        assert!(service.overrides_path.exists());
    }
}
