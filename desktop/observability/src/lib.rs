//! Shilpo internal process observability and opt-in profiling infrastructure.

pub mod discovery;
pub mod export;
pub mod paths;
pub mod role;
pub mod subscriber;

pub use discovery::{
    CompletedTraceMeta, TelemetrySummary, discover_newest_completed_trace,
    is_valid_completed_trace_file, summarize_profiles,
};
pub use export::{ExportError, ExportReport, export_trace};
pub use paths::{is_profile_enabled, resolve_profile_dir};
pub use role::ProcessRole;
pub use subscriber::{ObservabilityError, ObservabilityGuard, init, reset_initialized_for_testing};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_process_role_parse_and_as_str() {
        assert_eq!(ProcessRole::Shell.as_str(), "shell");
        assert_eq!(ProcessRole::Settings.as_str(), "settings");
        assert_eq!(ProcessRole::ExtensionHost.as_str(), "extension-host");
        assert_eq!(ProcessRole::DeviceDaemon.as_str(), "device-daemon");
        assert_eq!(ProcessRole::ThemeDaemon.as_str(), "theme-daemon");

        assert_eq!(ProcessRole::parse("shell"), Some(ProcessRole::Shell));
        assert_eq!(
            ProcessRole::parse("extension-host"),
            Some(ProcessRole::ExtensionHost)
        );
        assert_eq!(ProcessRole::parse("invalid"), None);
    }

    #[test]
    fn test_enablement_values() {
        unsafe {
            std::env::set_var("SHILPO_PROFILE", "1");
        }
        assert!(is_profile_enabled());

        unsafe {
            std::env::set_var("SHILPO_PROFILE", "true");
        }
        assert!(is_profile_enabled());

        unsafe {
            std::env::set_var("SHILPO_PROFILE", "TRUE");
        }
        assert!(is_profile_enabled());

        unsafe {
            std::env::set_var("SHILPO_PROFILE", "0");
        }
        assert!(!is_profile_enabled());

        unsafe {
            std::env::set_var("SHILPO_PROFILE", "false");
        }
        assert!(!is_profile_enabled());

        unsafe {
            std::env::set_var("SHILPO_PROFILE", "arbitrary");
        }
        assert!(!is_profile_enabled());

        unsafe {
            std::env::remove_var("SHILPO_PROFILE");
        }
        assert!(!is_profile_enabled());
    }

    #[test]
    fn test_relative_profile_dir_rejected() {
        unsafe {
            std::env::set_var("SHILPO_PROFILE_DIR", "relative/path");
        }
        let result = resolve_profile_dir();
        unsafe {
            std::env::remove_var("SHILPO_PROFILE_DIR");
        }
        assert!(matches!(
            result,
            Err(ObservabilityError::InvalidProfileDir(_))
        ));
    }

    #[test]
    fn test_active_filename_format() {
        let name = paths::generate_active_filename(ProcessRole::Shell, 12345);
        assert!(name.starts_with("shell-12345-"));
        assert!(name.ends_with(".json.part"));

        let final_name = paths::active_to_final_filename(&name);
        assert!(final_name.ends_with(".json"));
        assert!(!final_name.ends_with(".part"));
    }

    #[test]
    fn test_discovery_ignores_part_and_malformed() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        // Active trace (.part)
        std::fs::write(dir.join("shell-100-20260101T000000Z-uuid1.json.part"), "[]").unwrap();
        // Malformed trace (not valid json)
        std::fs::write(
            dir.join("shell-101-20260101T000000Z-uuid2.json"),
            "not json",
        )
        .unwrap();
        // Non-array trace
        std::fs::write(
            dir.join("shell-102-20260101T000000Z-uuid3.json"),
            "{\"foo\":\"bar\"}",
        )
        .unwrap();

        // Valid completed trace
        let valid_path = dir.join("shell-103-20260101T000000Z-uuid4.json");
        std::fs::write(&valid_path, "[]").unwrap();

        let discovered = discover_newest_completed_trace(dir).unwrap();
        assert_eq!(
            discovered.path.file_name().unwrap(),
            "shell-103-20260101T000000Z-uuid4.json"
        );
        assert_eq!(discovered.role, Some(ProcessRole::Shell));
    }

    #[test]
    fn test_export_success_and_refuses_overwrite() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        let source = dir.join("shell-100-20260101T000000Z-uuid1.json");
        std::fs::write(&source, "[{\"ph\":\"X\"}]").unwrap();

        let output = dir.join("exported_trace.json");
        let report = export_trace(Some(&source), &output, dir).unwrap();

        assert_eq!(report.bytes, 12);
        assert_eq!(report.process_role, "shell");
        assert_eq!(
            std::fs::read_to_string(&output).unwrap(),
            "[{\"ph\":\"X\"}]"
        );

        // Attempt export to existing file
        let err = export_trace(Some(&source), &output, dir).unwrap_err();
        assert!(matches!(err, ExportError::OutputExists(_)));
    }

    #[test]
    fn test_summary_telemetry_counts() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        std::fs::write(dir.join("shell-100-20260101T000000Z-uuid1.json.part"), "[]").unwrap();
        std::fs::write(dir.join("shell-101-20260101T000000Z-uuid2.json"), "[]").unwrap();

        let summary = summarize_profiles(dir).unwrap();
        assert_eq!(summary.completed_count, 1);
        assert_eq!(summary.incomplete_count, 1);
        assert!(summary.newest_completed.is_some());
    }
}
