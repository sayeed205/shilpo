use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_cli_scaffold_rust_bar_widget() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("my-rust-widget");

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args([
            "ext",
            "new",
            "My Rust Widget",
            target.to_str().unwrap(),
            "--language",
            "rust",
            "--contribution",
            "bar-widget",
        ])
        .output()
        .expect("shilpo ext new should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Created extension 'My Rust Widget'"));
    assert!(stdout.contains("Next steps:"));
    assert!(stdout.contains("shilpo ext build"));
    assert!(stdout.contains("shilpo ext dev"));

    assert!(target.join("extension.toml").exists());
    assert!(target.join("shilpo-ext.json").exists());
    assert!(target.join("Cargo.toml").exists());
    assert!(target.join("src/lib.rs").exists());
    assert!(target.join(".gitignore").exists());
    assert!(target.join("README.md").exists());
}

#[test]
fn test_cli_scaffold_create_alias_typescript_settings_page_pnpm() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("ts-settings");

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args([
            "ext",
            "create",
            "TS Settings",
            target.to_str().unwrap(),
            "--language",
            "typescript",
            "--contribution",
            "settings-page",
            "--package-manager",
            "pnpm",
        ])
        .output()
        .expect("shilpo ext create should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Created extension 'TS Settings'"));
    assert!(stdout.contains("pnpm install"));

    assert!(target.join("extension.toml").exists());
    assert!(target.join("shilpo-ext.json").exists());
    assert!(target.join("package.json").exists());
    assert!(target.join("tsconfig.json").exists());
    assert!(target.join(".npmrc").exists());
    assert!(target.join("src/extension.ts").exists());
    assert!(target.join("settings.schema.json").exists());
    assert!(target.join(".gitignore").exists());
    assert!(target.join("README.md").exists());
}

#[test]
fn test_cli_scaffold_quiet_mode() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("quiet-ext");

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args([
            "--quiet",
            "ext",
            "new",
            "Quiet Extension",
            target.to_str().unwrap(),
            "--language",
            "rust",
            "--contribution",
            "empty",
        ])
        .output()
        .expect("shilpo ext new --quiet should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), target.to_str().unwrap());
}

#[test]
fn test_cli_scaffold_json_mode() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("json-ext");

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args([
            "--json",
            "ext",
            "new",
            "JSON Extension",
            target.to_str().unwrap(),
            "--language",
            "typescript",
            "--contribution",
            "side-panel",
            "--extension-id",
            "io.github.test.json-ext",
            "--package-name",
            "json-ext-pkg",
            "--description",
            "A test extension in JSON mode",
        ])
        .output()
        .expect("shilpo --json ext new should run");

    assert!(output.status.success());
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("output must be valid JSON");
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "ext");
    assert_eq!(envelope["data"]["name"], "JSON Extension");
    assert_eq!(envelope["data"]["extension_id"], "io.github.test.json-ext");
    assert_eq!(envelope["data"]["language"], "typescript");
    assert_eq!(envelope["data"]["contribution"], "side-panel");
    assert_eq!(envelope["error"], serde_json::Value::Null);
}

#[test]
fn test_cli_scaffold_missing_flags_in_json_mode_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["--json", "ext", "new", "Missing Flags"])
        .output()
        .expect("shilpo --json ext new should run");

    assert_eq!(output.status.code(), Some(2));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("output must be valid JSON");
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["ok"], false);
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("missing required arguments")
    );
}

#[test]
fn test_cli_scaffold_target_collision_fails() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("collision");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("existing.txt"), "hello").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args([
            "--json",
            "ext",
            "new",
            "Collision Ext",
            target.to_str().unwrap(),
            "--language",
            "rust",
            "--contribution",
            "empty",
        ])
        .output()
        .expect("shilpo ext new should run");

    assert_eq!(output.status.code(), Some(2));
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("output must be valid JSON");
    assert_eq!(envelope["ok"], false);
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("already exists and is not empty")
    );
}

#[test]
fn test_cli_scaffold_with_capabilities_and_subscriptions() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("caps-subs");

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args([
            "ext",
            "new",
            "Caps Subs",
            target.to_str().unwrap(),
            "--language",
            "typescript",
            "--contribution",
            "action",
            "--capability",
            r#"{"kind":"network:http","hosts":["api.github.com"],"paths":["/repos/*"]}"#,
            "--subscribe",
            "theme_changed",
            "--subscribe",
            "outputs_changed",
        ])
        .output()
        .expect("shilpo ext new with caps/subs should run");

    assert!(output.status.success());
    let manifest_str = fs::read_to_string(target.join("extension.toml")).unwrap();
    assert!(manifest_str.contains("api.github.com"));
    assert!(manifest_str.contains("theme_changed"));
    assert!(manifest_str.contains("outputs_changed"));
    assert!(manifest_str.contains("events:subscribe"));
}
