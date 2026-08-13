use std::collections::BTreeMap;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use tempfile::TempDir;

use semver::Version;
use shilpo_ext_api::{CapabilityKind, ContributionId, ExtensionId, ViewLimits};

use crate::CatalogPaths;
use crate::worker::protocol::{ContributionSurface, ExtensionRuntimeKind};
use crate::worker::protocol::{
    ContributionDescriptor, ExtensionGeneration, ExtensionSnapshot, ScriptExtensionStatus,
};

use super::manager::{ScriptClock, ScriptRuntime};
use super::manifest::{
    ScriptBarWidgetContribution, ScriptContributions, ScriptManifest, ScriptMode,
    ScriptRuntimeConfig,
};
use super::record::{MAX_RECORD_BYTES, decode_and_validate_record};
use super::runner::{
    ProcessOutput, ProcessRunner, RealProcessRunner, ScriptProcessError, StreamProcess,
};

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
        version: Version::parse("0.1.0").unwrap(),
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

    assert!(ScriptManifest::from_toml("version = 'not-semver'").is_err());
}

#[test]
fn script_manifest_numeric_boundaries_duplicates_and_symlink_escape_are_exact() {
    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("run");
    File::create(&executable).unwrap();
    let mut manifest = ScriptManifest {
        schema_version: 1,
        id: ExtensionId::new("local.script.bounds").unwrap(),
        name: "Bounds".into(),
        version: Version::parse("0.1.0").unwrap(),
        runtime: ScriptRuntimeConfig {
            mode: ScriptMode::Poll,
            executable: "run".into(),
            args: Vec::new(),
            interval_ms: Some(1_000),
            timeout_ms: 100,
        },
        contributions: ScriptContributions {
            bar_widgets: vec![ScriptBarWidgetContribution {
                id: ContributionId::new("widget").unwrap(),
                name: "Widget".into(),
                description: None,
            }],
        },
    };
    assert!(manifest.validate(temp.path()).is_ok());
    manifest.runtime.interval_ms = Some(86_400_000);
    manifest.runtime.timeout_ms = 60_000;
    assert!(manifest.validate(temp.path()).is_ok());
    for interval in [999, 86_400_001] {
        manifest.runtime.interval_ms = Some(interval);
        assert!(manifest.validate(temp.path()).is_err());
    }
    manifest.runtime.interval_ms = Some(1_000);
    for timeout in [99, 60_001] {
        manifest.runtime.timeout_ms = timeout;
        assert!(manifest.validate(temp.path()).is_err());
    }
    manifest.runtime.timeout_ms = 100;
    manifest
        .contributions
        .bar_widgets
        .push(manifest.contributions.bar_widgets[0].clone());
    assert!(manifest.validate(temp.path()).is_err());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = temp.path().parent().unwrap().join("outside-script");
        File::create(&outside).unwrap();
        symlink(&outside, temp.path().join("escape")).unwrap();
        manifest.contributions.bar_widgets.truncate(1);
        manifest.runtime.executable = "escape".into();
        assert!(manifest.validate(temp.path()).is_err());
        let _ = fs::remove_file(outside);
    }
}

#[test]
fn checked_in_script_schema_matches_generator() {
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../schema/script-manifest-v1.schema.json"
    ))
    .unwrap();
    let generated = serde_json::to_value(schemars::schema_for!(ScriptManifest)).unwrap();
    assert_eq!(expected, generated);
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
        version: Version::parse("0.1.0").unwrap(),
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
    assert!(tree.validate(ViewLimits::default()).is_ok());

    // Full view record decoding
    let view_json = r#"{"schema_version":1,"contribution":"temperature","kind":"view","view":{"root":{"kind":"text","content":"45°C"}}}"#;
    let (contrib2, tree2) = decode_and_validate_record(view_json.as_bytes(), &manifest).unwrap();
    assert_eq!(contrib2.as_str(), "temperature");
    assert!(tree2.validate(ViewLimits::default()).is_ok());

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

    assert!(decode_and_validate_record(&[0xff, 0xfe], &manifest).is_err());
    let invalid_icon = r#"{"schema_version":1,"contribution":"temperature","kind":"text","text":"45°C","icon":"../../escape"}"#;
    assert!(decode_and_validate_record(invalid_icon.as_bytes(), &manifest).is_err());
    let unknown_field = r#"{"schema_version":1,"contribution":"temperature","kind":"text","text":"45°C","surprise":true}"#;
    assert!(decode_and_validate_record(unknown_field.as_bytes(), &manifest).is_err());
    let trailing_value = r#"{"schema_version":1,"contribution":"temperature","kind":"text","text":"45°C"} {}"#;
    assert!(decode_and_validate_record(trailing_value.as_bytes(), &manifest).is_err());
}

// -----------------------------------------------------------------------------
// 3. Fake Process Runner & Script Runtime Tests
// -----------------------------------------------------------------------------

struct FakeProcessRunner {
    poll_outputs: Mutex<BTreeMap<PathBuf, ProcessOutput>>,
    stream_lines: Mutex<BTreeMap<PathBuf, Vec<String>>>,
    poll_calls: AtomicUsize,
    stream_spawns: AtomicUsize,
}

impl FakeProcessRunner {
    fn new() -> Self {
        Self {
            poll_outputs: Mutex::new(BTreeMap::new()),
            stream_lines: Mutex::new(BTreeMap::new()),
            poll_calls: AtomicUsize::new(0),
            stream_spawns: AtomicUsize::new(0),
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
    fn next_line(
        &mut self,
        _timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<Option<Vec<u8>>, ScriptProcessError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(ScriptProcessError::Cancelled);
        }
        if self.index < self.lines.len() {
            let line = self.lines[self.index].clone();
            self.index += 1;
            Ok(Some(line.into_bytes()))
        } else {
            Ok(None)
        }
    }

    fn kill_group(&mut self) -> Result<(), ScriptProcessError> {
        Ok(())
    }

    fn stderr_excerpt(&mut self) -> String {
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
        cancelled: Arc<AtomicBool>,
    ) -> Result<ProcessOutput, ScriptProcessError> {
        self.poll_calls.fetch_add(1, Ordering::AcqRel);
        if cancelled.load(Ordering::Acquire) {
            return Err(ScriptProcessError::Cancelled);
        }
        if let Some(out) = self.poll_outputs.lock().unwrap().get(executable) {
            Ok(out.clone())
        } else {
            Err(ScriptProcessError::Spawn(
                "fake runner executable not found".into(),
            ))
        }
    }

    fn spawn_stream(
        &self,
        executable: &Path,
        _args: &[String],
        _cwd: &Path,
    ) -> Result<Box<dyn StreamProcess>, ScriptProcessError> {
        self.stream_spawns.fetch_add(1, Ordering::AcqRel);
        if let Some(lines) = self.stream_lines.lock().unwrap().get(executable) {
            Ok(Box::new(FakeStreamProcess {
                lines: lines.clone(),
                index: 0,
            }))
        } else {
            Err(ScriptProcessError::Spawn(
                "fake stream executable not found".into(),
            ))
        }
    }
}

struct TestClock(Mutex<std::time::Instant>);

impl TestClock {
    fn new() -> Self {
        Self(Mutex::new(std::time::Instant::now()))
    }

    fn advance(&self, duration: Duration) {
        let mut now = self.0.lock().unwrap();
        *now += duration;
    }
}

impl ScriptClock for TestClock {
    fn now(&self) -> std::time::Instant {
        *self.0.lock().unwrap()
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

    let clock = Arc::new(TestClock::new());
    let mut runtime =
        ScriptRuntime::with_dependencies(paths.clone(), fake_runner.clone(), clock.clone());
    runtime.reconcile(&[]);
    for _ in 0..100 {
        runtime.tick();
        if !runtime.views().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }

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

    clock.advance(Duration::from_secs(5));
    for _ in 0..100 {
        runtime.tick();
        if runtime
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("sensor read error"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let views_after_failure = runtime.views();
    assert_eq!(
        views_after_failure.len(),
        1,
        "last valid view must be retained on transient failure"
    );

    let diags = runtime.diagnostics();
    assert!(diags.iter().any(|d| d.contains("sensor read error")));
    assert_eq!(fake_runner.poll_calls.load(Ordering::Acquire), 2);

    // A malformed in-place replacement retains the last valid descriptor and view.
    fs::write(
        script_bundle_dir.join("manifest.toml"),
        manifest_toml.replace("timeout_ms = 1000", "timeout_ms = 90000"),
    )
    .unwrap();
    runtime.reconcile(&[]);
    assert_eq!(runtime.descriptors().len(), 1);
    assert_eq!(runtime.views().len(), 1);

    runtime.shutdown();
}

#[test]
fn polling_rejects_multiple_records_and_removal_cleans_snapshot_state() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path().join("config");
    let bundle = config_dir.join("scripts/multi");
    fs::create_dir_all(&bundle).unwrap();
    let manifest = r#"
schema_version = 1
id = "local.script.multi"
name = "Multi"
version = "0.1.0"
[runtime]
mode = "poll"
executable = "run"
interval_ms = 1000
timeout_ms = 100
[[contributions.bar_widgets]]
id = "widget"
name = "Widget"
"#;
    fs::write(bundle.join("manifest.toml"), manifest).unwrap();
    File::create(bundle.join("run")).unwrap();
    let runner = Arc::new(FakeProcessRunner::new());
    runner.set_poll_output(
        bundle.join("run"),
        ProcessOutput {
            exit_code: 0,
            stdout: b"{\"schema_version\":1,\"contribution\":\"widget\",\"kind\":\"text\",\"text\":\"one\"}\n{\"schema_version\":1,\"contribution\":\"widget\",\"kind\":\"text\",\"text\":\"two\"}\n".to_vec(),
            stderr: Vec::new(),
        },
    );
    let clock = Arc::new(TestClock::new());
    let mut runtime = ScriptRuntime::with_dependencies(
        CatalogPaths::new(temp.path().join("data"), config_dir),
        runner,
        clock,
    );
    runtime.reconcile(&[]);
    for _ in 0..100 {
        runtime.tick();
        if runtime
            .diagnostics()
            .iter()
            .any(|message| message.contains("multiple output records"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(runtime.views().is_empty());
    assert!(
        runtime
            .diagnostics()
            .iter()
            .any(|message| message.contains("multiple output records"))
    );

    fs::remove_dir_all(bundle).unwrap();
    runtime.reconcile(&[]);
    assert!(runtime.descriptors().is_empty());
    assert!(runtime.views().is_empty());
}

#[test]
fn streaming_consumes_records_and_restarts_after_eof() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path().join("config");
    let bundle = config_dir.join("scripts/stream");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(
        bundle.join("manifest.toml"),
        r#"
schema_version = 1
id = "local.script.stream"
name = "Stream"
version = "0.1.0"
[runtime]
mode = "stream"
executable = "run"
timeout_ms = 100
[[contributions.bar_widgets]]
id = "widget"
name = "Widget"
"#,
    )
    .unwrap();
    let executable = bundle.join("run");
    File::create(&executable).unwrap();
    let runner = Arc::new(FakeProcessRunner::new());
    runner.stream_lines.lock().unwrap().insert(
        executable,
        vec![
            r#"{"schema_version":1,"contribution":"widget","kind":"text","text":"streamed"}"#
                .into(),
        ],
    );
    let clock = Arc::new(TestClock::new());
    let mut runtime = ScriptRuntime::with_dependencies(
        CatalogPaths::new(temp.path().join("data"), config_dir),
        runner.clone(),
        clock.clone(),
    );
    runtime.reconcile(&[]);
    for _ in 0..100 {
        runtime.tick();
        if !runtime.views().is_empty()
            && runtime
                .diagnostics()
                .iter()
                .any(|message| message.contains("EOF"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(runtime.views().len(), 1);
    assert!(
        runtime
            .diagnostics()
            .iter()
            .any(|message| message.contains("EOF"))
    );
    clock.advance(Duration::from_millis(250));
    for _ in 0..100 {
        runtime.tick();
        if runner.stream_spawns.load(Ordering::Acquire) >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(runner.stream_spawns.load(Ordering::Acquire) >= 2);
    runtime.shutdown();
}

#[test]
fn duplicate_script_ids_name_both_sources_and_preserve_last_valid_source() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path().join("config");
    let scripts = config_dir.join("scripts");
    let manifest = |name: &str| {
        format!(
            r#"schema_version = 1
id = "local.script.duplicate"
name = "{name}"
version = "0.1.0"
[runtime]
mode = "poll"
executable = "run"
interval_ms = 1000
timeout_ms = 100
[[contributions.bar_widgets]]
id = "widget"
name = "Widget"
"#
        )
    };
    let first = scripts.join("first");
    fs::create_dir_all(&first).unwrap();
    fs::write(first.join("manifest.toml"), manifest("First")).unwrap();
    File::create(first.join("run")).unwrap();
    let mut runtime = ScriptRuntime::new(CatalogPaths::new(
        temp.path().join("data"),
        config_dir.clone(),
    ));
    runtime.reconcile(&[]);
    assert_eq!(runtime.descriptors().len(), 1);

    let second = scripts.join("second");
    fs::create_dir_all(&second).unwrap();
    fs::write(second.join("manifest.toml"), manifest("Second")).unwrap();
    File::create(second.join("run")).unwrap();
    runtime.reconcile(&[]);
    assert_eq!(runtime.descriptors().len(), 1);
    let diagnostic = runtime
        .diagnostics()
        .into_iter()
        .find(|message| message.contains("duplicate script extension ID"))
        .unwrap();
    assert!(diagnostic.contains("first") && diagnostic.contains("second"));
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
    runtime.reconcile(&active_wasm);

    assert!(
        runtime.descriptors().is_empty(),
        "conflicting script bundle must fail closed"
    );
    let diags = runtime.diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.contains("conflicts with an active WASM/catalog source"))
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
echo $! > child.pid
sleep 100
"#;
    fs::write(&script_path, script_content).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();

    let runner = RealProcessRunner;
    let start = std::time::Instant::now();
    let result = runner.run_poll(
        &script_path,
        &[],
        temp.path(),
        Duration::from_millis(200),
        Arc::new(AtomicBool::new(false)),
    );

    let elapsed = start.elapsed();
    assert!(result.is_err(), "script should time out");
    assert!(
        elapsed < Duration::from_secs(2),
        "timeout should terminate script cleanly"
    );
    let child_pid: i32 = fs::read_to_string(temp.path().join("child.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // SAFETY: signal 0 performs a liveness check and does not mutate the process.
    let alive = unsafe { libc::kill(child_pid, 0) } == 0;
    assert!(!alive, "the descendant process must be killed and reaped");
}

#[test]
fn poll_runner_drains_large_stdout_without_pipe_deadlock() {
    let temp = TempDir::new().unwrap();
    let script = temp.path().join("large-output.sh");
    fs::write(
        &script,
        "#!/bin/sh\nhead -c 2097152 /dev/zero\nprintf '\\n'\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).unwrap();
    let output = RealProcessRunner
        .run_poll(
            &script,
            &[],
            temp.path(),
            Duration::from_secs(2),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout.len(), MAX_RECORD_BYTES + 2);
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

    let capability_schema = serde_json::to_string(&schemars::schema_for!(
        shilpo_ext_api::Capability
    ))
    .unwrap()
    .to_lowercase();
    assert!(!capability_schema.contains("process:exec"));
    assert!(!capability_schema.contains("execprocess"));
}

#[test]
fn script_descriptor_view_and_status_survive_worker_snapshot_round_trip() {
    let extension_id = ExtensionId::new("local.script.snapshot").unwrap();
    let contribution_id = ContributionId::new("widget").unwrap();
    let canonical = shilpo_ext_api::CanonicalId::new(extension_id.clone(), contribution_id);
    let view = decode_and_validate_record(
        br#"{"schema_version":1,"contribution":"widget","kind":"text","text":"ready"}"#,
        &ScriptManifest {
            schema_version: 1,
            id: extension_id.clone(),
            name: "Snapshot".into(),
            version: Version::parse("0.1.0").unwrap(),
            runtime: ScriptRuntimeConfig {
                mode: ScriptMode::Poll,
                executable: "run".into(),
                args: Vec::new(),
                interval_ms: Some(1_000),
                timeout_ms: 100,
            },
            contributions: ScriptContributions {
                bar_widgets: vec![ScriptBarWidgetContribution {
                    id: ContributionId::new("widget").unwrap(),
                    name: "Widget".into(),
                    description: None,
                }],
            },
        },
    )
    .unwrap()
    .1;
    let snapshot = ExtensionSnapshot {
        generation: ExtensionGeneration(7),
        descriptors: Arc::from([ContributionDescriptor {
            id: canonical.clone(),
            extension_name: "Snapshot".into(),
            name: "Widget".into(),
            surface: ContributionSurface::Bar,
            runtime_kind: ExtensionRuntimeKind::TrustedLocalScript,
            settings_schema: None,
            default_size: None,
            minimum_size: None,
            bar_widget: None,
            action: None,
            default_binding: None,
        }]),
        views: Arc::new(BTreeMap::from([(canonical, view)])),
        script_extensions: Arc::from([ScriptExtensionStatus {
            id: extension_id,
            name: "Snapshot".into(),
            version: "0.1.0".into(),
            source: "local".into(),
            status: "ready".into(),
            contributions_count: 1,
            diagnostics: Vec::new(),
        }]),
        ..ExtensionSnapshot::default()
    };
    let encoded = serde_json::to_vec(&snapshot).unwrap();
    let decoded: ExtensionSnapshot = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded.descriptors[0].runtime_kind, ExtensionRuntimeKind::TrustedLocalScript);
    assert_eq!(decoded.views.len(), 1);
    assert_eq!(decoded.script_extensions[0].status, "ready");
}
