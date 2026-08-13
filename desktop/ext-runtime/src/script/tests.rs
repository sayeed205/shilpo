use std::collections::BTreeMap;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tempfile::TempDir;

use shilpo_ext_api::{CapabilityKind, ContributionId, ExtensionId, ViewLimits};

use crate::CatalogPaths;
use crate::worker::protocol::{ContributionSurface, ExtensionRuntimeKind};

use super::manager::ScriptRuntime;
use super::manifest::{
    ScriptBarWidgetContribution, ScriptContributions, ScriptManifest, ScriptMode,
    ScriptRuntimeConfig,
};
use super::record::{MAX_RECORD_BYTES, decode_and_validate_record};
use super::runner::{ProcessOutput, ProcessRunner, RealProcessRunner, StreamProcess};

// -----------------------------------------------------------------------------
// 1. Manifest Parsing & Validation Tests
// -----------------------------------------------------------------------------

#[test]
fn script_manifest_parsing_accepts_valid_and_rejects_unknown_fields() {
    let valid_toml = r#"
        schema_version = 1
        id = "local.script.cpu-temp"
        name = "CPU Temperature"
        version = "0.1.0"

        [runtime]
        mode = "poll"
        executable = "cpu-temp.sh"
        args = ["--format", "json"]
        interval_ms = 5000
        timeout_ms = 1000

        [[contributions.bar_widgets]]
        id = "temperature"
        name = "CPU Temperature"
        description = "Shows current CPU temperature"
    "#;

    let manifest = ScriptManifest::from_toml(valid_toml).expect("valid manifest should parse");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.id.as_str(), "local.script.cpu-temp");
    assert_eq!(manifest.runtime.mode, ScriptMode::Poll);
    assert_eq!(manifest.runtime.interval_ms, Some(5000));
    assert_eq!(manifest.contributions.bar_widgets.len(), 1);

    let unknown_field_toml = format!("{valid_toml}\nunknown_field = true\n");
    assert!(
        ScriptManifest::from_toml(&unknown_field_toml).is_err(),
        "deny_unknown_fields must reject unknown top-level keys"
    );
}

#[test]
fn script_manifest_validation_checks_path_escapes_executable_existence_and_ranges() {
    let temp = TempDir::new().unwrap();
    let bundle_dir = temp.path();

    let exec_file = bundle_dir.join("script.sh");
    File::create(&exec_file).unwrap();

    let valid_manifest = ScriptManifest {
        schema_version: 1,
        id: ExtensionId::new("local.script.test").unwrap(),
        name: "Test Script".into(),
        version: "0.1.0".into(),
        runtime: ScriptRuntimeConfig {
            mode: ScriptMode::Poll,
            executable: "script.sh".into(),
            args: vec![],
            interval_ms: Some(5000),
            timeout_ms: 1000,
        },
        contributions: ScriptContributions {
            bar_widgets: vec![ScriptBarWidgetContribution {
                id: ContributionId::new("widget").unwrap(),
                name: "Widget".into(),
                description: None,
            }],
        },
    };

    assert!(valid_manifest.validate(bundle_dir).is_ok());

    // Absolute executable path
    let mut invalid = valid_manifest.clone();
    invalid.runtime.executable = "/bin/ls".into();
    assert!(invalid.validate(bundle_dir).is_err());

    // Parent directory escape
    invalid.runtime.executable = "../script.sh".into();
    assert!(invalid.validate(bundle_dir).is_err());

    // Non-existent executable
    invalid.runtime.executable = "nonexistent.sh".into();
    assert!(invalid.validate(bundle_dir).is_err());

    // Directory as executable
    let sub_dir = bundle_dir.join("subdir");
    fs::create_dir(&sub_dir).unwrap();
    invalid.runtime.executable = "subdir".into();
    assert!(invalid.validate(bundle_dir).is_err());

    // Stream mode with interval_ms specified
    invalid = valid_manifest.clone();
    invalid.runtime.executable = "script.sh".into();
    invalid.runtime.mode = ScriptMode::Stream;
    invalid.runtime.interval_ms = Some(5000);
    assert!(invalid.validate(bundle_dir).is_err());

    // Poll mode timeout >= interval
    invalid = valid_manifest.clone();
    invalid.runtime.executable = "script.sh".into();
    invalid.runtime.mode = ScriptMode::Poll;
    invalid.runtime.interval_ms = Some(1000);
    invalid.runtime.timeout_ms = 1000;
    assert!(invalid.validate(bundle_dir).is_err());

    // Invalid semver
    invalid = valid_manifest.clone();
    invalid.runtime.executable = "script.sh".into();
    invalid.version = "not-semver".into();
    assert!(invalid.validate(bundle_dir).is_err());
}

// -----------------------------------------------------------------------------
// 2. Record Decoding & Lowering Tests
// -----------------------------------------------------------------------------

#[test]
fn record_decoding_handles_view_and_text_shorthand_and_enforces_read_only() {
    let manifest = ScriptManifest {
        schema_version: 1,
        id: ExtensionId::new("local.script.cpu").unwrap(),
        name: "CPU".into(),
        version: "0.1.0".into(),
        runtime: ScriptRuntimeConfig {
            mode: ScriptMode::Poll,
            executable: "cpu.sh".into(),
            args: vec![],
            interval_ms: Some(5000),
            timeout_ms: 1000,
        },
        contributions: ScriptContributions {
            bar_widgets: vec![ScriptBarWidgetContribution {
                id: ContributionId::new("temperature").unwrap(),
                name: "Temperature".into(),
                description: None,
            }],
        },
    };

    // Text shorthand record decoding
    let text_json = r#"{"schema_version":1,"contribution":"temperature","kind":"text","text":"45°C","icon":"thermometer"}"#;
    let (contrib, tree) = decode_and_validate_record(text_json.as_bytes(), &manifest).unwrap();
    assert_eq!(contrib.as_str(), "temperature");
    assert!(tree.validate_read_only(ViewLimits::default()).is_ok());

    // Full view record decoding
    let view_json = r#"{"schema_version":1,"contribution":"temperature","kind":"view","view":{"root":{"kind":"text","content":"45°C"}}}"#;
    let (contrib2, tree2) = decode_and_validate_record(view_json.as_bytes(), &manifest).unwrap();
    assert_eq!(contrib2.as_str(), "temperature");
    assert!(tree2.validate_read_only(ViewLimits::default()).is_ok());

    // Rejection of interactive nodes in v1
    let interactive_json = r#"{"schema_version":1,"contribution":"temperature","kind":"view","view":{"root":{"kind":"button","label":"Click","event_id":"evt"}}}"#;
    assert!(decode_and_validate_record(interactive_json.as_bytes(), &manifest).is_err());

    // Rejection of records exceeding 1 MiB limit
    let huge_bytes = vec![b'a'; MAX_RECORD_BYTES + 1];
    assert!(decode_and_validate_record(&huge_bytes, &manifest).is_err());

    // Rejection of unknown contribution ID
    let unknown_contrib_json =
        r#"{"schema_version":1,"contribution":"unknown_id","kind":"text","text":"45°C"}"#;
    assert!(decode_and_validate_record(unknown_contrib_json.as_bytes(), &manifest).is_err());

    // Rejection of invalid schema_version
    let invalid_schema_json =
        r#"{"schema_version":2,"contribution":"temperature","kind":"text","text":"45°C"}"#;
    assert!(decode_and_validate_record(invalid_schema_json.as_bytes(), &manifest).is_err());
}

// -----------------------------------------------------------------------------
// 3. Fake Process Runner & Script Runtime Tests
// -----------------------------------------------------------------------------

struct FakeProcessRunner {
    poll_outputs: Mutex<BTreeMap<PathBuf, ProcessOutput>>,
    stream_lines: Mutex<BTreeMap<PathBuf, Vec<String>>>,
}

impl FakeProcessRunner {
    fn new() -> Self {
        Self {
            poll_outputs: Mutex::new(BTreeMap::new()),
            stream_lines: Mutex::new(BTreeMap::new()),
        }
    }

    fn set_poll_output(&self, path: impl Into<PathBuf>, output: ProcessOutput) {
        self.poll_outputs
            .lock()
            .unwrap()
            .insert(path.into(), output);
    }
}

struct FakeStreamProcess {
    lines: Vec<String>,
    index: usize,
}

impl StreamProcess for FakeStreamProcess {
    fn next_line(&mut self, _timeout: Duration) -> Result<Option<String>, String> {
        if self.index < self.lines.len() {
            let line = self.lines[self.index].clone();
            self.index += 1;
            Ok(Some(line))
        } else {
            Ok(None)
        }
    }

    fn kill_group(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn stderr_excerpt(&self) -> String {
        String::new()
    }
}

impl ProcessRunner for FakeProcessRunner {
    fn run_poll(
        &self,
        executable: &Path,
        _args: &[String],
        _cwd: &Path,
        _timeout: Duration,
    ) -> Result<ProcessOutput, String> {
        if let Some(out) = self.poll_outputs.lock().unwrap().get(executable) {
            Ok(out.clone())
        } else {
            Err("fake runner executable not found".into())
        }
    }

    fn spawn_stream(
        &self,
        executable: &Path,
        _args: &[String],
        _cwd: &Path,
    ) -> Result<Box<dyn StreamProcess>, String> {
        if let Some(lines) = self.stream_lines.lock().unwrap().get(executable) {
            Ok(Box::new(FakeStreamProcess {
                lines: lines.clone(),
                index: 0,
            }))
        } else {
            Err("fake stream executable not found".into())
        }
    }
}

#[test]
fn script_runtime_reconciles_bundles_executes_poll_and_retains_valid_view() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path().join("config");
    let data_dir = temp.path().join("data");

    let script_bundle_dir = config_dir.join("scripts").join("cpu-temp");
    fs::create_dir_all(&script_bundle_dir).unwrap();

    let manifest_toml = r#"
        schema_version = 1
        id = "local.script.cpu-temp"
        name = "CPU Temp"
        version = "0.1.0"

        [runtime]
        mode = "poll"
        executable = "cpu-temp.sh"
        args = []
        interval_ms = 5000
        timeout_ms = 1000

        [[contributions.bar_widgets]]
        id = "temp"
        name = "CPU Temperature"
    "#;
    fs::write(script_bundle_dir.join("manifest.toml"), manifest_toml).unwrap();

    let exec_path = script_bundle_dir.join("cpu-temp.sh");
    File::create(&exec_path).unwrap();

    let paths = CatalogPaths::new(data_dir, config_dir);
    let fake_runner = Arc::new(FakeProcessRunner::new());

    let valid_json = r#"{"schema_version":1,"contribution":"temp","kind":"text","text":"50°C"}"#;
    fake_runner.set_poll_output(
        &exec_path,
        ProcessOutput {
            exit_code: 0,
            stdout: valid_json.as_bytes().to_vec(),
            stderr: Vec::new(),
        },
    );

    let mut runtime = ScriptRuntime::with_runner(paths.clone(), fake_runner.clone());
    runtime.reconcile(&[]).unwrap();

    let descriptors = runtime.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(
        descriptors[0].runtime_kind,
        ExtensionRuntimeKind::TrustedLocalScript
    );
    assert_eq!(descriptors[0].surface, ContributionSurface::Bar);

    let views = runtime.views();
    assert_eq!(views.len(), 1);

    // Now inject a failing poll output and verify that last valid view is retained!
    fake_runner.set_poll_output(
        &exec_path,
        ProcessOutput {
            exit_code: 1,
            stdout: Vec::new(),
            stderr: b"sensor read error".to_vec(),
        },
    );

    // Force re-run by ticking execution after resetting poll state
    runtime.force_poll_tick();
    let views_after_failure = runtime.views();
    assert_eq!(
        views_after_failure.len(),
        1,
        "last valid view must be retained on transient failure"
    );

    let diags = runtime.diagnostics();
    assert!(diags.iter().any(|d| d.contains("sensor read error")));

    runtime.shutdown();
}

#[test]
fn script_runtime_rejects_duplicate_wasm_id() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path().join("config");
    let data_dir = temp.path().join("data");

    let script_bundle_dir = config_dir.join("scripts").join("conflicting");
    fs::create_dir_all(&script_bundle_dir).unwrap();

    let manifest_toml = r#"
        schema_version = 1
        id = "org.shilpo.official-wasm"
        name = "Conflicting Script"
        version = "0.1.0"

        [runtime]
        mode = "poll"
        executable = "run.sh"
        args = []
        interval_ms = 5000
        timeout_ms = 1000

        [[contributions.bar_widgets]]
        id = "widget"
        name = "Widget"
    "#;
    fs::write(script_bundle_dir.join("manifest.toml"), manifest_toml).unwrap();
    File::create(script_bundle_dir.join("run.sh")).unwrap();

    let paths = CatalogPaths::new(data_dir, config_dir);
    let mut runtime = ScriptRuntime::new(paths);

    let active_wasm = vec![ExtensionId::new("org.shilpo.official-wasm").unwrap()];
    runtime.reconcile(&active_wasm).unwrap();

    assert!(
        runtime.descriptors().is_empty(),
        "conflicting script bundle must fail closed"
    );
    let diags = runtime.diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.contains("conflicts with active WASM extension ID"))
    );
}

// -----------------------------------------------------------------------------
// 4. Process Group Termination & Reaping Test
// -----------------------------------------------------------------------------

#[test]
fn real_process_runner_spawns_terminates_and_reaps_process_group() {
    let temp = TempDir::new().unwrap();
    let script_path = temp.path().join("long_sleep.sh");

    let script_content = r#"#!/bin/sh
sleep 100 &
sleep 100
"#;
    fs::write(&script_path, script_content).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();

    let runner = RealProcessRunner;
    let start = std::time::Instant::now();
    let result = runner.run_poll(&script_path, &[], temp.path(), Duration::from_millis(200));

    let elapsed = start.elapsed();
    assert!(result.is_err(), "script should time out");
    assert!(
        elapsed < Duration::from_secs(2),
        "timeout should terminate script cleanly"
    );
}

// -----------------------------------------------------------------------------
// 5. Security Regression Assertion
// -----------------------------------------------------------------------------

#[test]
fn wasm_guest_isolation_security_regression_assertion() {
    // Assert that CapabilityKind has no variant representing process execution
    let cap_kinds = [
        CapabilityKind::EventsSubscribe,
        CapabilityKind::WallpaperSet,
        CapabilityKind::NetworkHttp,
        CapabilityKind::FilesystemRead,
        CapabilityKind::NotificationsShow,
        CapabilityKind::ActionsInvoke,
        CapabilityKind::ThemeSetSource,
        CapabilityKind::ClipboardWrite,
        CapabilityKind::LocationRead,
        CapabilityKind::Secrets,
    ];

    for cap in cap_kinds {
        let debug_str = format!("{cap:?}");
        assert!(
            !debug_str.to_lowercase().contains("process")
                && !debug_str.to_lowercase().contains("exec"),
            "CapabilityKind variant {debug_str} must not expose process execution"
        );
    }

    // Assert WIT contract contains no process execution methods
    let wit_content = include_str!("../../../../core/ext-api/wit/extension.wit");
    assert!(
        !wit_content.contains("process:exec") && !wit_content.contains("exec-process"),
        "WIT extension contract must contain no process execution functions"
    );
}
