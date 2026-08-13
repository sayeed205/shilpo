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
pub use subscriber::{
    FilterError, LogFilterController, ObservabilityError, ObservabilityGuard, init,
    reset_initialized_for_testing,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

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
        let _guard = env_guard();
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
        let _guard = env_guard();
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
        std::fs::write(dir.join("unrelated.json"), "[]").unwrap();

        let discovered = discover_newest_completed_trace(dir).unwrap();
        assert_eq!(
            discovered.path.file_name().unwrap(),
            "shell-103-20260101T000000Z-uuid4.json"
        );
        assert_eq!(discovered.role, Some(ProcessRole::Shell));
    }

    #[test]
    fn test_role_inference_requires_filename_delimiter() {
        assert_eq!(discovery::infer_role_from_filename("shellevil.json"), None);
        assert_eq!(
            discovery::infer_role_from_filename("shell-1-trace.json"),
            Some(ProcessRole::Shell)
        );
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

    #[test]
    fn test_log_filter_controller_local_seam() {
        use tracing_subscriber::layer::SubscriberExt;
        let filter = tracing_subscriber::EnvFilter::builder().parse_lossy("warn");
        let (reload_layer, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
        let _subscriber = tracing_subscriber::registry().with(reload_layer);
        let controller = LogFilterController::new(reload_handle, "warn".into());

        assert_eq!(controller.current_filter(), "warn");

        // Valid update
        assert!(controller.set_filter("info,shilpo=debug").is_ok());
        assert_eq!(controller.current_filter(), "info,shilpo=debug");

        // Empty filter rejected
        assert_eq!(controller.set_filter("   "), Err(FilterError::EmptyFilter));
        assert_eq!(controller.current_filter(), "info,shilpo=debug");

        // Invalid filter syntax rejected
        assert!(matches!(
            controller.set_filter("invalid[[[syntax"),
            Err(FilterError::InvalidFilter(_))
        ));
        assert_eq!(controller.current_filter(), "info,shilpo=debug");
    }

    #[test]
    fn test_log_filter_controller_concurrency() {
        use tracing_subscriber::layer::SubscriberExt;
        let filter = tracing_subscriber::EnvFilter::builder().parse_lossy("warn");
        let (reload_layer, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
        let _subscriber = tracing_subscriber::registry().with(reload_layer);
        let controller = LogFilterController::new(reload_handle, "warn".into());

        let mut handles = vec![];
        for i in 0..10 {
            let c = controller.clone();
            handles.push(std::thread::spawn(move || {
                let directive = format!("info,target_{}=debug", i);
                let _ = c.set_filter(&directive);
                let current = c.current_filter();
                assert!(!current.is_empty());
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_environment_serialized_filter_restoration() {
        let _guard = env_guard();
        let old_env = std::env::var("RUST_LOG").ok();

        unsafe {
            std::env::set_var("RUST_LOG", "debug,shilpo_test=trace");
        }

        let filter_str = std::env::var("RUST_LOG")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "info".to_string());
        assert_eq!(filter_str, "debug,shilpo_test=trace");

        unsafe {
            if let Some(old) = old_env {
                std::env::set_var("RUST_LOG", old);
            } else {
                std::env::remove_var("RUST_LOG");
            }
        }
    }

    #[test]
    fn test_init_filter_reload_compositions_in_subprocess() {
        if std::env::var_os("SHILPO_INIT_FILTER_CHILD").is_some() {
            let guard = init(ProcessRole::Shell, "warn,shilpo=info").unwrap();
            let controller = guard.log_filter_controller().unwrap();
            assert_eq!(controller.current_filter(), "warn,shilpo=info");
            controller.set_filter("info,shilpo_test=debug").unwrap();
            assert_eq!(controller.current_filter(), "info,shilpo_test=debug");
            return;
        }

        let temp = TempDir::new().unwrap();
        for enabled in ["0", "1"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tests::test_init_filter_reload_compositions_in_subprocess",
                ])
                .env("SHILPO_INIT_FILTER_CHILD", "1")
                .env("SHILPO_PROFILE", enabled)
                .env("SHILPO_PROFILE_DIR", temp.path())
                .env("RUST_LOG", "warn,shilpo=info")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "subscriber child failed for SHILPO_PROFILE={enabled}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let completed = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .count();
        assert_eq!(completed, 1);
    }
}
