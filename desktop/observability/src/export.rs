use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::discovery::{discover_newest_completed_trace, is_valid_completed_trace_file};

/// Export outcome data payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportReport {
    pub source: PathBuf,
    pub output: PathBuf,
    pub bytes: u64,
    pub process_role: String,
}

/// Structured error for trace export operations.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("no completed traces found in '{0}'")]
    NoCompletedTraces(PathBuf),
    #[error("source trace '{path}' is invalid: {reason}")]
    InvalidSource { path: PathBuf, reason: String },
    #[error("output path '{0}' already exists; export will not overwrite existing files")]
    OutputExists(PathBuf),
    #[error("source path '{0}' and output path are identical")]
    SourceEqualsOutput(PathBuf),
    #[error("output parent directory '{0}' does not exist")]
    OutputParentMissing(PathBuf),
    #[error("failed to create output file '{path}': {source}")]
    CreateOutputFailed { path: PathBuf, source: io::Error },
    #[error("failed to write export file '{path}': {source}")]
    WriteOutputFailed { path: PathBuf, source: io::Error },
    #[error("failed to clean up temporary output file '{path}': {source}")]
    CleanupFailed { path: PathBuf, source: io::Error },
}

impl ExportError {
    pub fn stable_code(&self) -> &'static str {
        match self {
            Self::NoCompletedTraces(_) => "profile.export.no_completed_traces",
            Self::InvalidSource { .. } => "profile.export.invalid_source",
            Self::OutputExists(_) => "profile.export.output_exists",
            Self::SourceEqualsOutput(_) => "profile.export.source_equals_output",
            Self::OutputParentMissing(_) => "profile.export.output_parent_missing",
            Self::CreateOutputFailed { .. } => "profile.export.create_failed",
            Self::WriteOutputFailed { .. } => "profile.export.write_failed",
            Self::CleanupFailed { .. } => "profile.export.cleanup_failed",
        }
    }
}

/// Export a validated completed trace file to `output_path`.
pub fn export_trace(
    source_path: Option<&Path>,
    output_path: &Path,
    profile_dir: &Path,
) -> Result<ExportReport, ExportError> {
    let (source, bytes, role) = if let Some(src) = source_path {
        let (bytes, role) =
            is_valid_completed_trace_file(src).map_err(|reason| ExportError::InvalidSource {
                path: src.to_path_buf(),
                reason,
            })?;
        (src.to_path_buf(), bytes, role)
    } else {
        let meta = discover_newest_completed_trace(profile_dir)
            .map_err(|_| ExportError::NoCompletedTraces(profile_dir.to_path_buf()))?;
        (meta.path, meta.bytes, meta.role)
    };

    let abs_source = source.canonicalize().unwrap_or_else(|_| source.clone());
    let abs_output = if output_path.is_absolute() {
        output_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ExportError::OutputParentMissing(PathBuf::from(".")))?
            .join(output_path)
    };

    if abs_source
        == abs_output
            .canonicalize()
            .unwrap_or_else(|_| abs_output.clone())
    {
        return Err(ExportError::SourceEqualsOutput(source));
    }

    if abs_output.exists() {
        return Err(ExportError::OutputExists(abs_output.clone()));
    }

    let parent = abs_output
        .parent()
        .ok_or_else(|| ExportError::OutputParentMissing(abs_output.clone()))?;

    if !parent.as_os_str().is_empty() && !parent.exists() {
        return Err(ExportError::OutputParentMissing(parent.to_path_buf()));
    }

    let content = fs::read(&source).map_err(|err| ExportError::InvalidSource {
        path: source.clone(),
        reason: err.to_string(),
    })?;

    let mut out_file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&abs_output)
    {
        Ok(f) => f,
        Err(source) => {
            return Err(ExportError::CreateOutputFailed {
                path: abs_output.clone(),
                source,
            });
        }
    };

    if let Err(write_err) = out_file.write_all(&content) {
        if let Err(clean_err) = fs::remove_file(&abs_output) {
            return Err(ExportError::CleanupFailed {
                path: abs_output,
                source: clean_err,
            });
        }
        return Err(ExportError::WriteOutputFailed {
            path: abs_output.clone(),
            source: write_err,
        });
    }

    if let Err(flush_err) = out_file.flush() {
        if let Err(clean_err) = fs::remove_file(&abs_output) {
            return Err(ExportError::CleanupFailed {
                path: abs_output,
                source: clean_err,
            });
        }
        return Err(ExportError::WriteOutputFailed {
            path: abs_output,
            source: flush_err,
        });
    }

    if let Err(sync_err) = out_file.sync_all() {
        if let Err(clean_err) = fs::remove_file(&abs_output) {
            return Err(ExportError::CleanupFailed {
                path: abs_output.clone(),
                source: clean_err,
            });
        }
        return Err(ExportError::WriteOutputFailed {
            path: abs_output.clone(),
            source: sync_err,
        });
    }

    let role_str = role
        .map(|r| r.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(ExportReport {
        source: abs_source,
        output: abs_output,
        bytes,
        process_role: role_str,
    })
}
