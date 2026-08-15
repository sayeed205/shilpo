use std::path::Path;

#[test]
fn showcase_manifest_has_all_ten_contribution_families() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("extensions/example/extension.toml");

    let text = std::fs::read_to_string(&manifest_path).expect("read showcase extension.toml");
    let manifest = shilpo_ext_api::ExtensionManifest::from_toml(&text)
        .expect("showcase extension.toml must deserialize into valid ExtensionManifest");

    assert_eq!(manifest.id.as_str(), "org.shilpo.example");
    assert_eq!(manifest.version.to_string(), "0.1.0");

    let contribs = &manifest.contributions;
    assert_eq!(
        contribs.bar_widgets.len(),
        1,
        "bar_widgets contribution present"
    );
    assert_eq!(
        contribs.bar_menus.len(),
        1,
        "bar_menus contribution present"
    );
    assert_eq!(
        contribs.desktop_widgets.len(),
        1,
        "desktop_widgets contribution present"
    );
    assert_eq!(
        contribs.settings_pages.len(),
        1,
        "settings_pages contribution present"
    );
    assert_eq!(
        contribs.side_panels.len(),
        1,
        "side_panels contribution present"
    );
    assert_eq!(
        contribs.search_providers.len(),
        1,
        "search_providers contribution present"
    );
    assert_eq!(contribs.actions.len(), 1, "actions contribution present");
    assert_eq!(
        contribs.keyboard_shortcuts.len(),
        1,
        "keyboard_shortcuts contribution present"
    );
    assert_eq!(
        contribs.background_tasks.len(),
        1,
        "background_tasks contribution present"
    );
    assert_eq!(
        contribs.wallpaper_providers.len(),
        1,
        "wallpaper_providers contribution present"
    );
}

#[test]
fn showcase_settings_schema_is_valid_json() {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("extensions/example/settings.schema.json");

    let text = std::fs::read_to_string(&schema_path).expect("read settings.schema.json");
    let val: serde_json::Value =
        serde_json::from_str(&text).expect("settings.schema.json must be valid JSON");
    assert!(
        val.is_object(),
        "settings.schema.json must be a JSON object"
    );
    assert_eq!(
        val.get("title").and_then(|v| v.as_str()),
        Some("Showcase Extension Settings")
    );
}

#[test]
fn world_clock_manifest_is_valid() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("extensions/world-clock/extension.toml");

    let text = std::fs::read_to_string(&manifest_path).expect("read world-clock extension.toml");
    let manifest = shilpo_ext_api::ExtensionManifest::from_toml(&text)
        .expect("world-clock extension.toml must deserialize into valid ExtensionManifest");

    assert_eq!(manifest.id.as_str(), "io.github.alice.world-clock");
    assert!(!manifest.contributions.bar_widgets.is_empty());
}

#[test]
fn trusted_local_script_manifest_is_valid() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("extensions/cpu-temp-script/extension.toml");

    let text =
        std::fs::read_to_string(&manifest_path).expect("read cpu-temp-script extension.toml");
    let script_manifest: shilpo_ext_runtime::script::ScriptManifest =
        toml::from_str(&text).expect("cpu-temp-script must be valid ScriptManifest");

    let bundle = tempfile::tempdir().expect("create script validation bundle");
    let script_path = bundle.path().join("cpu-temp.sh");
    std::fs::write(
        &script_path,
        "#!/bin/sh\nprintf '%s' '{\"schema_version\":1,\"contribution\":\"cpu-temp\",\"kind\":\"text\",\"text\":\"42 C\"}'\n",
    )
        .expect("write script fixture");
    script_manifest
        .validate(bundle.path())
        .expect("script manifest must pass ScriptRuntime validation");

    let output = std::process::Command::new("sh")
        .arg(&script_path)
        .output()
        .expect("execute bounded script fixture");
    assert!(output.status.success());
    shilpo_ext_runtime::script::decode_and_validate_record(&output.stdout, &script_manifest)
        .expect("script record must decode and validate");

    assert_eq!(script_manifest.id.as_str(), "local.script.cpu-temp");
    assert_eq!(script_manifest.runtime.executable, "cpu-temp.sh");
}

#[test]
fn documentation_hub_files_exist() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let required_docs = [
        "docs/extensions/index.md",
        "docs/extensions/getting-started-typescript.md",
        "docs/extensions/getting-started-rust.md",
        "docs/extensions/manifest-reference.md",
        "docs/extensions/architecture-and-lifecycle.md",
        "docs/extensions/security-and-capabilities.md",
        "docs/extensions/trusted-local-scripts.md",
        "docs/extensions/testing-guide.md",
        "docs/extensions/troubleshooting-and-smoke.md",
        "docs/extensions/coverage-matrix.md",
        "extensions/example/COVERAGE.md",
        "extensions/example/README.md",
        "extensions/world-clock/README.md",
        "extensions/cpu-temp-script/README.md",
    ];

    for rel_path in &required_docs {
        let full_path = repo_root.join(rel_path);
        assert!(
            full_path.exists(),
            "Required documentation file must exist: {rel_path}"
        );
    }
}
