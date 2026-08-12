use crate::{ObservabilityError, ProcessRole};
use std::path::PathBuf;
use uuid::Uuid;

/// Check whether profiling is enabled via environment variable `SHILPO_PROFILE`.
pub fn is_profile_enabled() -> bool {
    match std::env::var("SHILPO_PROFILE") {
        Ok(val) => val == "1" || val.eq_ignore_ascii_case("true"),
        Err(_) => false,
    }
}

/// Resolve the directory where profile traces are stored.
pub fn resolve_profile_dir() -> Result<PathBuf, ObservabilityError> {
    if let Some(override_dir) = std::env::var("SHILPO_PROFILE_DIR")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let path = PathBuf::from(override_dir);
        if !path.is_absolute() {
            return Err(ObservabilityError::InvalidProfileDir(path));
        }
        return Ok(path);
    }

    if let Some(xdg_state) = std::env::var("XDG_STATE_HOME")
        .ok()
        .filter(|s| !s.is_empty())
    {
        return Ok(PathBuf::from(xdg_state).join("shilpo/profiles"));
    }

    if let Some(home) = std::env::var("HOME").ok().filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(home).join(".local/state/shilpo/profiles"));
    }

    Ok(PathBuf::from(".local/state/shilpo/profiles"))
}

/// Generate collision-resistant active `.json.part` trace filename.
pub fn generate_active_filename(role: ProcessRole, pid: u32) -> String {
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%dT%H%M%SZ");
    let uuid = Uuid::new_v4().simple();
    format!("{}-{}-{}-{}.json.part", role.as_str(), pid, timestamp, uuid)
}

/// Convert active `.json.part` filename to final `.json` filename.
pub fn active_to_final_filename(active_filename: &str) -> String {
    if let Some(stripped) = active_filename.strip_suffix(".part") {
        stripped.to_string()
    } else {
        active_filename.to_string()
    }
}
