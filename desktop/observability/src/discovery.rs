use crate::ProcessRole;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Metadata for a completed trace file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletedTraceMeta {
    pub path: PathBuf,
    pub role: Option<ProcessRole>,
    pub bytes: u64,
    pub modified_at: String,
}

/// Telemetry summary inventory for `shilpo doctor --telemetry`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetrySummary {
    pub profile_dir: PathBuf,
    pub profile_enabled: bool,
    pub completed_count: usize,
    pub completed_bytes: u64,
    pub incomplete_count: usize,
    pub incomplete_bytes: u64,
    pub newest_completed: Option<CompletedTraceMeta>,
    pub warnings: Vec<String>,
}

/// Check if a file is a valid completed trace file and return its size and role.
pub fn is_valid_completed_trace_file(path: &Path) -> Result<(u64, Option<ProcessRole>), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("not a regular file".into());
    }

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if file_name.ends_with(".part") || !file_name.ends_with(".json") {
        return Err("invalid trace filename format".into());
    }

    let bytes = meta.len();
    let content = fs::read_to_string(path).map_err(|e| format!("cannot read file: {e}"))?;
    let val: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("invalid JSON trace: {e}"))?;

    if !val.is_array() {
        return Err("trace root is not a JSON array".into());
    }

    let role = infer_role_from_filename(file_name);
    Ok((bytes, role))
}

pub fn infer_role_from_filename(file_name: &str) -> Option<ProcessRole> {
    [
        ProcessRole::Shell,
        ProcessRole::Settings,
        ProcessRole::ExtensionHost,
        ProcessRole::DeviceDaemon,
        ProcessRole::ThemeDaemon,
    ]
    .into_iter()
    .find(|role| {
        file_name
            .strip_prefix(role.as_str())
            .is_some_and(|rest| rest.starts_with('-'))
    })
}

/// Discover the newest valid completed trace file in `profile_dir`.
pub fn discover_newest_completed_trace(profile_dir: &Path) -> Result<CompletedTraceMeta, String> {
    let entries =
        fs::read_dir(profile_dir).map_err(|e| format!("cannot read profile directory: {e}"))?;
    let mut candidates = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read profile entry: {error}"))?;
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if file_name.ends_with(".part")
            || !file_name.ends_with(".json")
            || infer_role_from_filename(&file_name).is_none()
        {
            continue;
        }
        if let Ok((bytes, role)) = is_valid_completed_trace_file(&path) {
            let mtime = fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let time_str = chrono::DateTime::<chrono::Utc>::from(mtime).to_rfc3339();
            candidates.push((mtime, file_name, path, role, bytes, time_str));
        }
    }

    if candidates.is_empty() {
        return Err("no completed traces found".into());
    }

    // Sort by mtime desc, tie-break by file_name desc
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let (_mtime, _name, path, role, bytes, modified_at) = candidates.remove(0);
    Ok(CompletedTraceMeta {
        path,
        role,
        bytes,
        modified_at,
    })
}

/// Generate telemetry summary of profile traces in `profile_dir`.
pub fn summarize_profiles(profile_dir: &Path) -> Result<TelemetrySummary, String> {
    let profile_enabled = crate::paths::is_profile_enabled();
    if !profile_dir.exists() {
        return Ok(TelemetrySummary {
            profile_dir: profile_dir.to_path_buf(),
            profile_enabled,
            completed_count: 0,
            completed_bytes: 0,
            incomplete_count: 0,
            incomplete_bytes: 0,
            newest_completed: None,
            warnings: Vec::new(),
        });
    }

    let entries =
        fs::read_dir(profile_dir).map_err(|e| format!("cannot read profile directory: {e}"))?;

    let mut completed_count = 0;
    let mut completed_bytes = 0;
    let mut incomplete_count = 0;
    let mut incomplete_bytes = 0;
    let mut completed_candidates = Vec::new();
    let mut warnings = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!("cannot read profile entry: {error}"));
                continue;
            }
        };
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                warnings.push(format!("cannot read entry {file_name}: {e}"));
                continue;
            }
        };

        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }

        if file_name.ends_with(".part") {
            incomplete_count += 1;
            incomplete_bytes += meta.len();
        } else if file_name.ends_with(".json") && infer_role_from_filename(&file_name).is_some() {
            match is_valid_completed_trace_file(&path) {
                Ok((bytes, role)) => {
                    completed_count += 1;
                    completed_bytes += bytes;
                    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    let time_str = chrono::DateTime::<chrono::Utc>::from(mtime).to_rfc3339();
                    completed_candidates.push((mtime, file_name, path, role, bytes, time_str));
                }
                Err(err) => {
                    warnings.push(format!("malformed trace file '{file_name}': {err}"));
                }
            }
        }
    }

    completed_candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let newest_completed = if let Some((_mtime, _name, path, role, bytes, modified_at)) =
        completed_candidates.into_iter().next()
    {
        Some(CompletedTraceMeta {
            path,
            role,
            bytes,
            modified_at,
        })
    } else {
        None
    };

    Ok(TelemetrySummary {
        profile_dir: profile_dir.to_path_buf(),
        profile_enabled,
        completed_count,
        completed_bytes,
        incomplete_count,
        incomplete_bytes,
        newest_completed,
        warnings,
    })
}
