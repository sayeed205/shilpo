use std::path::PathBuf;
use std::process::Command;

#[test]
fn test_bench_script_interface_and_error_handling() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root");
    let script_path = repo_root.join("scripts/bench.sh");

    assert!(script_path.exists(), "scripts/bench.sh must exist");

    // 1. Test --help flag exits with status 0
    let help_output = Command::new(&script_path)
        .arg("--help")
        .current_dir(&repo_root)
        .output()
        .expect("execute bench.sh --help");

    assert!(help_output.status.success());
    let stdout = String::from_utf8_lossy(&help_output.stdout);
    assert!(stdout.contains("Shilpo Benchmark Suite Runner"));
    assert!(stdout.contains("core"));
    assert!(stdout.contains("config"));
    assert!(stdout.contains("wasm"));
    assert!(stdout.contains("smoke"));
    assert!(stdout.contains("all"));

    // 2. Test unknown suite exits with status 1
    let error_output = Command::new(&script_path)
        .arg("unknown-invalid-suite")
        .current_dir(&repo_root)
        .output()
        .expect("execute bench.sh with invalid suite");

    assert!(!error_output.status.success());
    let stderr = String::from_utf8_lossy(&error_output.stderr);
    assert!(stderr.contains("Unknown benchmark suite 'unknown-invalid-suite'"));
}
