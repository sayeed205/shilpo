use std::fs;

use shilpo::config::{ConfigResolver, ShellConfig};
use tempfile::TempDir;

const TOML_FIXTURE: &str = include_str!("../fixtures/config/valid_full.toml");

#[test]
fn test_benchmark_config_fixture_integrity_and_coverage() {
    let config: ShellConfig =
        toml::from_str(TOML_FIXTURE).expect("valid_full.toml must deserialize cleanly");

    // Semantic validation check
    config
        .validate()
        .expect("valid_full.toml must pass semantic validation");

    // Ensure all required sections are present and non-empty
    assert_eq!(config.version, 1);
    assert!(!config.theme.font_family.is_empty());
    assert!(config.bar.height > 0);
    assert!(!config.bar.widgets.start.is_empty());
    assert!(!config.bar.widgets.center.is_empty());
    assert!(!config.bar.widgets.end.is_empty());
    assert!(!config.extensions.settings.is_empty());
    assert!(!config.outputs.is_empty());
    assert!(!config.startup.autostart_apps.is_empty());
    assert!(!config.capture.default_selection.is_empty());
    assert!(!config.keybindings.is_empty());
}

#[test]
fn test_benchmark_config_fixture_layered_resolution() {
    let dir = TempDir::new().expect("create temp dir");
    let base_path = dir.path().join("config.toml");
    fs::write(&base_path, TOML_FIXTURE).expect("write primary config");

    let conf_d = dir.path().join("conf.d");
    fs::create_dir_all(&conf_d).expect("create conf.d");
    fs::write(
        conf_d.join("10-theme.toml"),
        "version = 1\n[theme]\nfont_family = \"Fira Code\"\n",
    )
    .expect("write 10-theme.toml");
    fs::write(
        conf_d.join("20-outputs.toml"),
        "version = 1\n[outputs.DP-1]\nenabled = true\nscale = 1.25\n",
    )
    .expect("write 20-outputs.toml");

    let overrides_path = dir.path().join("overrides.toml");
    fs::write(&overrides_path, "[theme]\ncorner_radius_scale = 1.2\n")
        .expect("write overrides.toml");

    let resolver = ConfigResolver::new(dir.path());
    let (snapshot, report) = resolver
        .resolve_initial()
        .expect("initial layered resolution must succeed");

    assert_eq!(snapshot.config.version, 1);
    assert_eq!(snapshot.config.theme.font_family, "Fira Code");
    assert!((snapshot.config.theme.corner_radius_scale - 1.2).abs() < f32::EPSILON);
    assert!(snapshot.config.outputs.contains_key("DP-1"));
    assert!(report.sources_loaded.len() >= 3);
}
