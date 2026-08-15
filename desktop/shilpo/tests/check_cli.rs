use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_cli_ext_check_typescript_fixture() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("sdk/typescript/tests/fixture");

    let wasm_file = fixture_dir.join("extension.wasm");
    if !wasm_file.exists() {
        // Compile component via jco if not yet built
        let status = Command::new("npx")
            .args([
                "--yes",
                "@bytecodealliance/jco@1",
                "componentize",
                fixture_dir.join("extension.ts").to_str().unwrap(),
                "--wit",
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("core/ext-api/wit")
                    .to_str()
                    .unwrap(),
                "--world-name",
                "extension",
                "--backend",
                "qjs",
                "--backend-qjs-disable-async",
                "-o",
                wasm_file.to_str().unwrap(),
            ])
            .status()
            .expect("jco componentize should run");
        assert!(status.success(), "jco componentize must succeed");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["ext", "check", fixture_dir.to_str().unwrap()])
        .output()
        .expect("shilpo ext check should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("info[manifest.valid]: 'org.shilpo.ts-fixture' 0.1.0"));
    assert!(stdout.contains("info[wasm.valid]: component interface validated at extension.wasm"));
}

#[test]
fn test_cli_ext_check_json_output() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("sdk/typescript/tests/fixture");

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
    assert_eq!(envelope["data"]["extension_id"], "org.shilpo.ts-fixture");
    assert!(envelope["data"]["diagnostics"].is_array());
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
    assert!(combined.contains("error[file.missing]"));
}
