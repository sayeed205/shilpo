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

#[test]
fn test_stable_benchmark_groups_remain_documented() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root");
    let docs = std::fs::read_to_string(repo_root.join("docs/benchmarks.md"))
        .expect("read benchmark documentation");

    let benchmark_sources = [
        (
            "core/ext-api/benches/identity.rs",
            &[
                "identity/extension_id/valid",
                "identity/contribution_id/invalid",
                "identity/canonical_id/new",
            ][..],
        ),
        (
            "core/ext-api/benches/view_tree.rs",
            &["view_tree/validate_valid", "view_tree/validate_rejection"][..],
        ),
        (
            "desktop/shilpo/benches/config.rs",
            &[
                "config/deserialize",
                "config/validate",
                "config/parse_and_validate",
                "config/resolve_layered",
            ][..],
        ),
        (
            "desktop/ext-runtime/benches/wasm.rs",
            &["wasm/cold_load"][..],
        ),
    ];

    for (source_path, groups) in benchmark_sources {
        let source = std::fs::read_to_string(repo_root.join(source_path))
            .unwrap_or_else(|error| panic!("read {source_path}: {error}"));
        for group in groups {
            assert!(
                source.contains(group),
                "stable benchmark group `{group}` must remain in {source_path}"
            );
            assert!(
                docs.contains(group),
                "stable benchmark group `{group}` must remain documented"
            );
        }
    }
}
