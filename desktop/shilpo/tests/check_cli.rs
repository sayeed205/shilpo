use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;

#[test]
fn test_cli_ext_check_component_fixture() {
    let fixture_dir = copy_component_fixture();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["ext", "check", fixture_dir.to_str().unwrap()])
        .output()
        .expect("shilpo ext check should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("info[manifest.valid]: 'org.shilpo.check-fixture' 0.1.0"));
    assert!(stdout.contains("info[wasm.valid]: component interface validated at extension.wasm"));
}

#[test]
fn test_cli_ext_check_json_output() {
    let fixture_dir = copy_component_fixture();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["--json", "ext", "check", fixture_dir.to_str().unwrap()])
        .output()
        .expect("shilpo ext check --json should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value =
        serde_json::from_str(&stdout).expect("output must be valid JSON");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "ext");
    assert_eq!(envelope["data"]["extension_id"], "org.shilpo.check-fixture");
    assert!(envelope["data"]["diagnostics"].is_array());
}

fn copy_component_fixture() -> PathBuf {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("desktop/ext-runtime/tests/fixtures/bar-menu-component");
    let dir = tempdir().expect("temporary fixture directory");
    let path = dir.keep();
    std::fs::write(
        path.join("extension.toml"),
        "schema_version = 1\nid = \"org.shilpo.check-fixture\"\nname = \"Check Fixture\"\nversion = \"0.1.0\"\n\n[library]\npath = \"extension.wasm\"\n",
    )
    .expect("fixture manifest should be writable");
    std::fs::copy(
        source_root.join("target/wasm32-wasip2/release/bar_menu_component_fixture.wasm"),
        path.join("extension.wasm"),
    )
    .expect("checked-in component fixture should be available");
    path
}

#[test]
fn test_cli_ext_check_invalid_wasm() {
    let temp = tempdir().unwrap();
    let dir = temp.path();

    fs::write(
        dir.join("extension.toml"),
        r#"
            id = "dev.local.broken"
            name = "Broken"
            version = "0.1.0"

            [library]
            path = "extension.wasm"
        "#,
    )
    .unwrap();
    fs::write(dir.join("extension.wasm"), b"NOT_A_WASM_FILE").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["ext", "check", dir.to_str().unwrap()])
        .output()
        .expect("shilpo ext check should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}\n{stderr}");
    assert!(combined.contains("error[wasm.invalid]"));
}

#[test]
fn test_cli_ext_check_missing_manifest() {
    let temp = tempdir().unwrap();
    let dir = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["ext", "check", dir.to_str().unwrap()])
        .output()
        .expect("shilpo ext check should run");

    assert!(!output.status.success());
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("error[manifest.missing]") || combined.contains("manifest.missing"));
}
