use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

#[test]
fn test_cli_ext_lint_valid_extension() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test Extension"
version = "0.1.0"
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["ext", "lint", dir.path().to_str().unwrap()])
        .output()
        .expect("shilpo ext lint should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Lint passed with 0 errors, 0 warnings"));
    assert!(stdout.contains("manifest.valid"));
}

#[test]
fn test_cli_ext_lint_warnings_and_deny_warnings() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test Extension"
version = "0.1.0"

[[capabilities]]
kind = "network:http"
hosts = ["*"]
"#,
    )
    .unwrap();

    // Default: warnings do not fail lint (exit 0)
    let output_allow = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["ext", "lint", dir.path().to_str().unwrap()])
        .output()
        .expect("shilpo ext lint should run");

    assert!(output_allow.status.success());
    let stdout_allow = String::from_utf8_lossy(&output_allow.stdout);
    assert!(stdout_allow.contains("warning: [capability.broad-network-scope]"));

    // With --deny-warnings: warnings fail lint (exit 1)
    let output_deny = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args([
            "ext",
            "lint",
            dir.path().to_str().unwrap(),
            "--deny-warnings",
        ])
        .output()
        .expect("shilpo ext lint should run");

    assert_eq!(output_deny.status.code(), Some(1));
}

#[test]
fn test_cli_ext_lint_json_output() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test Extension"
version = "0.1.0"
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["--json", "ext", "lint", dir.path().to_str().unwrap()])
        .output()
        .expect("shilpo ext lint --json should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value =
        serde_json::from_str(&stdout).expect("output must be valid JSON");

    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "ext");
    assert_eq!(envelope["data"]["schema_version"], 1);
    assert_eq!(envelope["data"]["extension_id"], "org.shilpo.test");
    assert_eq!(envelope["data"]["passed"], true);
    assert_eq!(envelope["data"]["error_count"], 0);
    assert!(envelope["data"]["diagnostics"].is_array());
}

#[test]
fn test_cli_ext_lint_quiet_mode() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test Extension"
version = "0.1.0"
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["--quiet", "ext", "lint", dir.path().to_str().unwrap()])
        .output()
        .expect("shilpo ext lint --quiet should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().is_empty());
}

#[test]
fn test_cli_ext_lint_example_extension() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("extensions/example");

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["ext", "lint", example_path.to_str().unwrap()])
        .output()
        .expect("shilpo ext lint on extensions/example should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Lint passed with 0 errors, 0 warnings"));
}

#[test]
fn test_cli_ext_lint_non_existent_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["ext", "lint", "non_existent_path_123456"])
        .output()
        .expect("shilpo ext lint should run");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stderr.contains("path.not-found") || stdout.contains("path.not-found"));
}

#[test]
fn test_cli_ext_lint_json_on_failing_extension() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 99
id = "org.shilpo.test"
name = "Test Extension"
version = "0.1.0"
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args(["--json", "ext", "lint", dir.path().to_str().unwrap()])
        .output()
        .expect("shilpo ext lint --json should run");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value =
        serde_json::from_str(&stdout).expect("output must be valid JSON");

    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["command"], "ext");
    assert_eq!(envelope["data"]["passed"], false);
    assert!(envelope["data"]["error_count"].as_u64().unwrap() >= 1);
}

#[test]
fn test_cli_ext_lint_passes_timeout_to_wasm_validation() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("extension.toml"),
        r#"
schema_version = 1
id = "org.shilpo.test"
name = "Test Extension"
version = "0.1.0"

[library]
path = "extension.wasm"
"#,
    )
    .unwrap();
    fs::write(dir.path().join("extension.wasm"), b"not a component").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_shilpo"))
        .args([
            "--timeout",
            "1ms",
            "ext",
            "lint",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("shilpo ext lint should run with a timeout");

    assert_eq!(output.status.code(), Some(1));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("wasm.invalid") || combined.contains("wasm.timeout"));
}
