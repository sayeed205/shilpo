use crate::daemon::DaemonState;
use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub fn state_file_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"))
        .join("shilpo")
        .join("colors.json")
}

pub fn write_state_snapshot(state: &DaemonState) -> Result<PathBuf> {
    write_state_snapshot_to(state, &state_file_path())
}

pub fn write_state_snapshot_to(state: &DaemonState, path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .context("Invalid state file parent directory")?;

    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create state directory: {}", parent.display()))?;

    let temp_name = format!(
        "colors.json.tmp.{}.{}.{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        uuid::Uuid::new_v4().simple()
    );
    let temp_path = parent.join(temp_name);

    let json =
        serde_json::to_string_pretty(state).context("Failed to serialize DaemonState to JSON")?;

    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp_path)
            .with_context(|| format!("Failed to open temp state file: {}", temp_path.display()))?;

        file.write_all(json.as_bytes()).with_context(|| {
            format!(
                "Failed to write state to temp file: {}",
                temp_path.display()
            )
        })?;

        file.sync_all()
            .with_context(|| format!("Failed to sync temp state file: {}", temp_path.display()))?;
    }

    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;

    if let Ok(dir_file) = File::open(parent) {
        let _ = dir_file.sync_all();
    }

    Ok(path.to_path_buf())
}

pub fn read_state_snapshot_from(path: &Path) -> Option<DaemonState> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn read_state_snapshot() -> Option<DaemonState> {
    read_state_snapshot_from(&state_file_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_atomic_persistence_and_permissions() {
        let state = DaemonState {
            theme: shilpo_ui::theme::ThemeState {
                revision: 42,
                ..Default::default()
            },
            ..Default::default()
        };

        let path = write_state_snapshot(&state).expect("Failed to write snapshot");
        assert!(path.exists());

        let metadata = fs::metadata(&path).unwrap();
        let permissions = metadata.permissions();
        assert_eq!(permissions.mode() & 0o777, 0o600);

        let read_back = read_state_snapshot().expect("Failed to read snapshot");
        assert_eq!(read_back.theme.revision, 42);
        assert_eq!(read_back, state);
    }
}
