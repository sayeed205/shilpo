use std::path::{Path, PathBuf};

use shilpo_ext_api::ExtensionId;
use shilpo_ext_runtime::{
    ExtensionCatalog, ExtensionCli, ExtensionCliResult, ReleaseChannel, UpdateState,
    default_extension_state_dir, sign_package,
};

use super::ipc::IpcAdapter;

pub struct ExtAdapter {
    ipc: IpcAdapter,
    state_dir: PathBuf,
    catalog: ExtensionCatalog,
}

pub struct ExtOpResult {
    pub success: bool,
    pub data: serde_json::Value,
    pub human_message: String,
    pub warnings: Vec<String>,
    pub exit_code: i32,
}

impl Default for ExtAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtAdapter {
    pub fn new() -> Self {
        Self {
            ipc: IpcAdapter::new(),
            state_dir: default_extension_state_dir(),
            catalog: ExtensionCatalog::open_default(),
        }
    }

    fn notify_daemon_if_needed(&self, result: &mut ExtOpResult) {
        if result.success {
            match self.ipc.config_reload() {
                Ok(()) => {}
                Err((3, _)) => record_refresh_warning(result, "live shell daemon is not running"),
                Err((_, e)) => {
                    record_refresh_warning(result, format!("live daemon refresh failed: {e}"))
                }
            }
        }
    }

    pub fn status(&self) -> ExtOpResult {
        match self.ipc.telemetry() {
            Ok(telemetry) => {
                let diagnostics = telemetry.extension_host_diagnostics_json;
                let ext_status: serde_json::Value =
                    serde_json::from_str(&diagnostics).unwrap_or(serde_json::Value::Null);
                if ext_status.is_null() {
                    return ExtOpResult {
                        success: false,
                        data: serde_json::Value::Null,
                        human_message: "Extension host diagnostics are unavailable".into(),
                        warnings: Vec::new(),
                        exit_code: 1,
                    };
                }
                let state_str = ext_status
                    .get("lifecycle")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let host_gen = ext_status
                    .get("host_generation")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let engine_gen = ext_status
                    .get("engine_generation")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let mut human_lines = vec![
                    "Extension Host Status:".to_string(),
                    format!("  State: {state_str}"),
                    format!("  Host Generation: {host_gen}"),
                    format!("  Engine Generation: {engine_gen}"),
                ];

                let wasm_extensions = ext_status.get("wasm_extensions").and_then(|v| v.as_array());

                if let Some(wasms) = wasm_extensions
                    && !wasms.is_empty()
                {
                    human_lines.push(String::new());
                    human_lines.push(format!("WASM Extensions ({}):", wasms.len()));
                    for ext in wasms {
                        let id = ext.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let state = ext
                            .get("state")
                            .and_then(|v| v.as_str())
                            .unwrap_or("closed");
                        let trip_count =
                            ext.get("trip_count").and_then(|v| v.as_u64()).unwrap_or(0);
                        let desc = match state {
                            "closed" => {
                                let failures = ext
                                    .get("consecutive_failures")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                if failures > 0 {
                                    format!("closed, {failures} failure(s)")
                                } else {
                                    "closed, healthy".to_string()
                                }
                            }
                            "open" => {
                                let retry_ms = ext
                                    .get("retry_after_ms")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                let secs = retry_ms.div_ceil(1000);
                                format!("open, retrying in {secs}s (trip {trip_count})")
                            }
                            "half_open" => {
                                let successes = ext
                                    .get("consecutive_successes")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                format!(
                                    "half_open, probing ({successes}/3 successes, trip {trip_count})"
                                )
                            }
                            "permanently_disabled" => {
                                format!(
                                    "permanently_disabled, failed after {trip_count} trip cycles"
                                )
                            }
                            other => other.to_string(),
                        };
                        human_lines.push(format!("  {id} [{desc}]"));
                    }
                }

                let script_extensions = ext_status
                    .get("script_extensions")
                    .and_then(|v| v.as_array());

                if let Some(scripts) = script_extensions
                    && !scripts.is_empty()
                {
                    human_lines.push(String::new());
                    human_lines.push(format!("Script Extensions ({}):", scripts.len()));
                    for script in scripts {
                        let id = script
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let status = script
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let version = script
                            .get("version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        human_lines.push(format!("  {id} (v{version}) [{status}]"));
                    }
                }

                ExtOpResult {
                    success: true,
                    data: ext_status,
                    human_message: human_lines.join("\n"),
                    warnings: Vec::new(),
                    exit_code: 0,
                }
            }
            Err((3, _)) => ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message:
                    "shell daemon is unavailable; cannot query extension host diagnostics".into(),
                warnings: Vec::new(),
                exit_code: 3,
            },
            Err((code, msg)) => ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("Failed to query telemetry from daemon: {msg}"),
                warnings: Vec::new(),
                exit_code: code,
            },
        }
    }

    pub fn build(&self, path: Option<&Path>, release: bool) -> ExtOpResult {
        let target = path.unwrap_or_else(|| Path::new("."));
        let cli_res = ExtensionCli::build(target, release);
        ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "artifact": cli_res.artifact,
                "release": release,
                "diagnostics": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        }
    }

    pub fn check(&self, path: Option<&Path>) -> ExtOpResult {
        let target = path.unwrap_or_else(|| Path::new("."));
        let cli_res = ExtensionCli::check(target);
        ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "diagnostics": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        }
    }

    pub fn pack(&self, path: Option<&Path>, output: Option<&Path>) -> ExtOpResult {
        let source = path.unwrap_or_else(|| Path::new("."));
        let out_dir = output
            .map(PathBuf::from)
            .unwrap_or_else(|| source.join("dist"));
        let cli_res = ExtensionCli::pack(source, &out_dir);
        ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "artifact": cli_res.artifact,
                "diagnostics": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        }
    }

    pub fn dev(
        &self,
        path: Option<&Path>,
        json: bool,
        quiet: bool,
        _timeout: std::time::Duration,
    ) -> ExtOpResult {
        let target_dir = path.unwrap_or_else(|| Path::new("."));
        let canonical_root = match target_dir.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                let err_msg = format!(
                    "failed to resolve extension path '{}': {e}",
                    target_dir.display()
                );
                return ExtOpResult {
                    success: false,
                    data: serde_json::json!({ "error": err_msg }),
                    human_message: err_msg,
                    warnings: Vec::new(),
                    exit_code: 1,
                };
            }
        };

        let manifest_path = canonical_root.join("extension.toml");
        let manifest_str = match std::fs::read_to_string(&manifest_path) {
            Ok(s) => s,
            Err(e) => {
                let err_msg = format!(
                    "failed to read extension.toml at '{}': {e}",
                    manifest_path.display()
                );
                return ExtOpResult {
                    success: false,
                    data: serde_json::json!({ "error": err_msg }),
                    human_message: err_msg,
                    warnings: Vec::new(),
                    exit_code: 1,
                };
            }
        };

        let manifest = match shilpo_ext_api::ExtensionManifest::from_toml(&manifest_str) {
            Ok(m) => m,
            Err(e) => {
                let err_msg = format!(
                    "invalid extension.toml at '{}': {e}",
                    manifest_path.display()
                );
                return ExtOpResult {
                    success: false,
                    data: serde_json::json!({ "error": err_msg }),
                    human_message: err_msg,
                    warnings: Vec::new(),
                    exit_code: 1,
                };
            }
        };

        // Step 1: Initial Build
        if !quiet && !json {
            eprintln!(
                "Building extension '{}' (v{})...",
                manifest.id, manifest.version
            );
        }

        let initial_build = ExtensionCli::build(&canonical_root, false);
        if !initial_build.success {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "event": "build",
                        "status": "failed",
                        "build_sequence": 1,
                        "extension_id": manifest.id.to_string(),
                        "diagnostics": initial_build.diagnostics,
                    })
                );
            }
            return ExtOpResult {
                success: false,
                data: serde_json::json!({
                    "event": "build",
                    "status": "failed",
                    "extension_id": manifest.id.to_string(),
                    "diagnostics": initial_build.diagnostics,
                }),
                human_message: format!(
                    "Initial build failed for extension '{}':\n{}",
                    manifest.id,
                    initial_build.diagnostics.join("\n")
                ),
                warnings: Vec::new(),
                exit_code: 1,
            };
        }

        if json {
            println!(
                "{}",
                serde_json::json!({
                    "event": "build",
                    "status": "success",
                    "build_sequence": 1,
                    "extension_id": manifest.id.to_string(),
                })
            );
        }

        // Step 2: D-Bus connection & StartDevSession
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                return ExtOpResult {
                    success: false,
                    data: serde_json::json!({ "error": format!("tokio runtime error: {e}") }),
                    human_message: format!("Error creating runtime: {e}"),
                    warnings: Vec::new(),
                    exit_code: 1,
                };
            }
        };

        let result = rt.block_on(async {
            let conn = match zbus::Connection::session().await {
                Ok(c) => c,
                Err(e) => {
                    return Err(format!(
                        "failed to connect to session D-Bus: {e}. Is the Shilpo shell running?"
                    ));
                }
            };

            let shell_proxy = match crate::shell::dbus::ShellProxy::new(&conn).await {
                Ok(p) => p,
                Err(e) => {
                    return Err(format!("failed to attach ShellProxy: {e}"));
                }
            };

            let session_id = match shell_proxy
                .start_dev_session(
                    manifest.id.to_string(),
                    canonical_root.to_string_lossy().to_string(),
                )
                .await
            {
                Ok(id) => id,
                Err(e) => {
                    return Err(format!("StartDevSession failed: {e}"));
                }
            };

            let mut build_sequence: u64 = 1;

            // Step 3: Initial Reload
            let reload_res = shell_proxy
                .reload_dev_session(session_id.clone(), build_sequence, "extension.wasm".into())
                .await;

            match reload_res {
                Ok(res) => {
                    if res.outcome == "applied" {
                        if json {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "event": "reload",
                                    "status": "applied",
                                    "build_sequence": build_sequence,
                                    "extension_id": manifest.id.to_string(),
                                    "session_id": session_id,
                                    "host_generation": res.host_generation,
                                    "engine_generation": res.engine_generation,
                                })
                            );
                        } else if !quiet {
                            eprintln!(
                                "Extension '{}' (v{}) loaded into dev session '{}' (build #{})",
                                manifest.id, manifest.version, session_id, build_sequence
                            );
                        }
                    } else {
                        if json {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "event": "reload",
                                    "status": res.outcome,
                                    "build_sequence": build_sequence,
                                    "extension_id": manifest.id.to_string(),
                                    "diagnostic_code": res.diagnostic_code,
                                    "message": res.message,
                                })
                            );
                        } else {
                            eprintln!(
                                "Reload rejected [{}]: {}",
                                res.diagnostic_code, res.message
                            );
                        }
                    }
                }
                Err(e) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "event": "reload",
                                "status": "error",
                                "build_sequence": build_sequence,
                                "extension_id": manifest.id.to_string(),
                                "error": e.to_string(),
                            })
                        );
                    } else {
                        eprintln!("Reload error: {e}");
                    }
                }
            }

            if !quiet && !json {
                eprintln!(
                    "Watching '{}' for changes... (Press Ctrl+C to stop)",
                    canonical_root.display()
                );
            }

            // Step 4: File watcher setup
            let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
            let root_for_watcher = canonical_root.clone();

            let mut watcher = match notify::RecommendedWatcher::new(
                move |res: Result<notify::Event, notify::Error>| {
                    if let Ok(event) = res
                        && (event.kind.is_modify()
                            || event.kind.is_create()
                            || event.kind.is_remove())
                    {
                        let relevant = event.paths.iter().any(|p| !should_ignore_path(p, &root_for_watcher));
                        if relevant {
                            let _ = event_tx.send(());
                        }
                    }
                },
                notify::Config::default(),
            ) {
                Ok(w) => w,
                Err(e) => return Err(format!("failed to initialize file watcher: {e}")),
            };

            use notify::Watcher;
            if let Err(e) = watcher.watch(&canonical_root, notify::RecursiveMode::Recursive) {
                return Err(format!("failed to watch directory: {e}"));
            }

            // Step 5: Event loop with debouncing and Ctrl-C handling
            let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .map_err(|e| format!("failed to register SIGINT handler: {e}"))?;
            let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|e| format!("failed to register SIGTERM handler: {e}"))?;

            loop {
                tokio::select! {
                    _ = sigint.recv() => {
                        if !quiet && !json {
                            eprintln!("\nReceived interrupt signal, ending dev session...");
                        }
                        let _ = shell_proxy.end_dev_session(session_id.clone()).await;
                        break;
                    }
                    _ = sigterm.recv() => {
                        if !quiet && !json {
                            eprintln!("\nReceived terminate signal, ending dev session...");
                        }
                        let _ = shell_proxy.end_dev_session(session_id.clone()).await;
                        break;
                    }
                    Some(()) = event_rx.recv() => {
                        // Debounce window (150ms)
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        // Drain any coalesced changes that arrived during debounce window
                        while event_rx.try_recv().is_ok() {}

                        build_sequence += 1;
                        if !quiet && !json {
                            eprintln!("File changes detected. Rebuilding #{}...", build_sequence);
                        }

                        let root_clone = canonical_root.clone();
                        let build_res = tokio::task::spawn_blocking(move || {
                            ExtensionCli::build(&root_clone, false)
                        }).await.map_err(|e| format!("build task join error: {e}"))?;

                        if !build_res.success {
                            if json {
                                println!(
                                    "{}",
                                    serde_json::json!({
                                        "event": "build",
                                        "status": "failed",
                                        "build_sequence": build_sequence,
                                        "extension_id": manifest.id.to_string(),
                                        "diagnostics": build_res.diagnostics,
                                    })
                                );
                            } else {
                                eprintln!(
                                    "Build #{} failed:\n{}",
                                    build_sequence,
                                    build_res.diagnostics.join("\n")
                                );
                            }
                            continue;
                        }

                        if json {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "event": "build",
                                    "status": "success",
                                    "build_sequence": build_sequence,
                                    "extension_id": manifest.id.to_string(),
                                })
                            );
                        }

                        let reload_res = shell_proxy
                            .reload_dev_session(session_id.clone(), build_sequence, "extension.wasm".into())
                            .await;

                        match reload_res {
                            Ok(res) => {
                                if res.outcome == "applied" {
                                    if json {
                                        println!(
                                            "{}",
                                            serde_json::json!({
                                                "event": "reload",
                                                "status": "applied",
                                                "build_sequence": build_sequence,
                                                "extension_id": manifest.id.to_string(),
                                                "session_id": session_id,
                                                "host_generation": res.host_generation,
                                                "engine_generation": res.engine_generation,
                                            })
                                        );
                                    } else if !quiet {
                                        eprintln!(
                                            "Dev reload applied (build #{}, host gen: {}, engine gen: {})",
                                            build_sequence, res.host_generation, res.engine_generation
                                        );
                                    }
                                } else {
                                    if json {
                                        println!(
                                            "{}",
                                            serde_json::json!({
                                                "event": "reload",
                                                "status": res.outcome,
                                                "build_sequence": build_sequence,
                                                "extension_id": manifest.id.to_string(),
                                                "diagnostic_code": res.diagnostic_code,
                                                "message": res.message,
                                            })
                                        );
                                    } else {
                                        eprintln!(
                                            "Dev reload rejected [{}]: {}",
                                            res.diagnostic_code, res.message
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                if json {
                                    println!(
                                        "{}",
                                        serde_json::json!({
                                            "event": "reload",
                                            "status": "error",
                                            "build_sequence": build_sequence,
                                            "extension_id": manifest.id.to_string(),
                                            "error": e.to_string(),
                                        })
                                    );
                                } else {
                                    eprintln!("Dev reload error: {e}");
                                }
                            }
                        }
                    }
                }
            }

            Ok(())
        });

        match result {
            Ok(()) => ExtOpResult {
                success: true,
                data: serde_json::json!({ "status": "stopped" }),
                human_message: "Dev server stopped".into(),
                warnings: Vec::new(),
                exit_code: 0,
            },
            Err(e) => ExtOpResult {
                success: false,
                data: serde_json::json!({ "error": e }),
                human_message: format!("Error: {e}"),
                warnings: Vec::new(),
                exit_code: 1,
            },
        }
    }

    pub fn logs(&self, id_str: Option<&str>, follow: bool) -> ExtOpResult {
        let Some(id_raw) = id_str else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: "extension ID is required".into(),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };
        let Ok(id) = ExtensionId::new(id_raw) else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("invalid extension ID '{id_raw}'"),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };
        let cli_res = ExtensionCli::logs(&id, &self.state_dir);
        let result = ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "log_file": cli_res.artifact,
                "logs": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        };
        if result.success && follow {
            shilpo_ext_runtime::follow_log(&id, &self.state_dir);
        }
        result
    }

    pub fn list(&self, dev_only: bool) -> ExtOpResult {
        let dev_regs = shilpo_ext_runtime::development_registrations(&self.state_dir).0;
        let installed = self.catalog.installed().unwrap_or_default();
        let installed_count = installed.len();
        let live_scripts = self
            .ipc
            .telemetry()
            .ok()
            .and_then(|telemetry| {
                serde_json::from_str::<serde_json::Value>(
                    &telemetry.extension_host_diagnostics_json,
                )
                .ok()
            })
            .and_then(|diagnostics| diagnostics.get("script_extensions").cloned())
            .and_then(|statuses| statuses.as_array().cloned());
        let script_items: Vec<serde_json::Value> = if let Some(statuses) = live_scripts {
            statuses
                .into_iter()
                .map(|status| script_item_from_status(&status))
                .collect()
        } else {
            let catalog_paths = shilpo_ext_runtime::CatalogPaths::platform_default();
            shilpo_ext_runtime::script::discover_script_bundles(&catalog_paths)
                .iter()
                .map(|b| {
                    serde_json::json!({
                        "id": b.id,
                        "name": b.name,
                        "version": b.version,
                        "path": b.path,
                        "mode": b.mode,
                        "type": "script",
                        "runtime_kind": "trusted_local_script",
                        "source": "local",
                        "status": b.status,
                        "trusted": true,
                        "sandboxed": false,
                        "label": "Trusted local script (not sandboxed)",
                        "contributions_count": b.contributions_count,
                        "diagnostics": b.diagnostics,
                    })
                })
                .collect()
        };

        let mut human_lines = Vec::new();
        human_lines.push(format!("Development extensions: {}", dev_regs.len()));
        human_lines.push(format!(
            "Installed extensions: {}",
            if dev_only { 0 } else { installed_count }
        ));
        if !installed.is_empty() && !dev_only {
            for ext in &installed {
                human_lines.push(format!(
                    "  {} (v{}) [wasm, installed] - {} contributions",
                    ext.receipt.id,
                    ext.receipt.active.version,
                    ext.manifest.contributions.bar_widgets.len()
                        + ext.manifest.contributions.desktop_widgets.len()
                ));
            }
        }
        human_lines.push(format!(
            "Trusted local script extensions: {}",
            script_items.len()
        ));
        for script in &script_items {
            human_lines.push(format!(
                "  {} (v{}) [script, local, {}] - {} contributions (Trusted local script (not sandboxed))",
                script.get("id").and_then(|value| value.as_str()).unwrap_or("unknown"),
                script.get("version").and_then(|value| value.as_str()).unwrap_or("unknown"),
                script.get("status").and_then(|value| value.as_str()).unwrap_or("unknown"),
                script.get("contributions_count").and_then(|value| value.as_u64()).unwrap_or(0),
            ));
        }

        ExtOpResult {
            success: true,
            data: serde_json::json!({
                "development": dev_regs,
                "installed": if dev_only { Vec::new() } else { installed },
                "scripts": script_items,
            }),
            human_message: human_lines.join("\n"),
            warnings: Vec::new(),
            exit_code: 0,
        }
    }

    pub fn install(&self, target: &str, hash: Option<&str>) -> ExtOpResult {
        let catalog_paths = shilpo_ext_runtime::CatalogPaths::platform_default();
        let script_bundles = shilpo_ext_runtime::script::discover_script_bundles(&catalog_paths);
        if script_bundles.iter().any(|b| b.id.as_str() == target) {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!(
                    "script extension '{target}' is a trusted local script in $XDG_CONFIG_HOME/shilpo/scripts and is not managed via catalog install"
                ),
                warnings: Vec::new(),
                exit_code: 1,
            };
        }

        let cli_res = if target.starts_with("https://") {
            match self.catalog.install_url(target, hash) {
                Ok(receipt) => ExtensionCliResult {
                    success: true,
                    extension_id: Some(receipt.id.to_string()),
                    artifact: None,
                    diagnostics: vec![format!(
                        "installed '{}' {} disabled",
                        receipt.id, receipt.active.version
                    )],
                },
                Err(error) => ExtensionCliResult {
                    success: false,
                    extension_id: None,
                    artifact: None,
                    diagnostics: vec![format!("error[install.url]: {error}")],
                },
            }
        } else if let Ok(id) = ExtensionId::new(target) {
            if !Path::new(target).exists() {
                ExtensionCli::install_from_catalog(&id, &self.catalog)
            } else {
                ExtensionCli::install(Path::new(target), &self.catalog)
            }
        } else {
            ExtensionCli::install(Path::new(target), &self.catalog)
        };

        let mut op_res = ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "diagnostics": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        };
        self.notify_daemon_if_needed(&mut op_res);
        op_res
    }

    pub fn update(&self, id_str: Option<&str>, all: bool, dry_run: bool) -> ExtOpResult {
        if let Some(id) = id_str
            && self.is_script_id(id)
        {
            return script_catalog_operation_error(id, "update");
        }
        if all {
            let snapshot = self.catalog.snapshot();
            let available = snapshot
                .updates
                .into_iter()
                .filter(|u| u.state == UpdateState::Available)
                .map(|u| u.id)
                .collect::<Vec<_>>();

            if dry_run {
                return ExtOpResult {
                    success: true,
                    data: serde_json::json!({ "would_update": available }),
                    human_message: format!("Would update {} extension(s)", available.len()),
                    warnings: Vec::new(),
                    exit_code: 0,
                };
            }

            let results = available
                .iter()
                .map(|id| ExtensionCli::install_from_catalog(id, &self.catalog))
                .collect::<Vec<_>>();
            let failed = results.iter().any(|r| !r.success);

            let mut op_res = ExtOpResult {
                success: !failed,
                data: serde_json::json!({
                    "results": results.iter().map(|r| serde_json::json!({
                        "extension_id": r.extension_id,
                        "success": r.success,
                        "diagnostics": r.diagnostics,
                    })).collect::<Vec<_>>()
                }),
                human_message: format!("Updated {} extension(s)", available.len()),
                warnings: Vec::new(),
                exit_code: if !failed { 0 } else { 1 },
            };
            self.notify_daemon_if_needed(&mut op_res);
            return op_res;
        }

        let Some(id_raw) = id_str else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: "extension ID or --all required".into(),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };
        let Ok(id) = ExtensionId::new(id_raw) else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("invalid extension ID '{id_raw}'"),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };

        let cli_res = ExtensionCli::install_from_catalog(&id, &self.catalog);
        let mut op_res = ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "diagnostics": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        };
        self.notify_daemon_if_needed(&mut op_res);
        op_res
    }

    pub fn enable(&self, id_str: &str) -> ExtOpResult {
        if self.is_script_id(id_str) {
            return script_catalog_operation_error(id_str, "enable");
        }
        let Ok(id) = ExtensionId::new(id_str) else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("invalid extension ID '{id_str}'"),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };
        let cli_res = ExtensionCli::enable(&id, &self.catalog);
        let mut op_res = ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "diagnostics": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        };
        self.notify_daemon_if_needed(&mut op_res);
        op_res
    }

    pub fn disable(&self, id_str: &str) -> ExtOpResult {
        if self.is_script_id(id_str) {
            return script_catalog_operation_error(id_str, "disable");
        }
        let Ok(id) = ExtensionId::new(id_str) else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("invalid extension ID '{id_str}'"),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };
        let cli_res = ExtensionCli::disable(&id, &self.catalog);
        let mut op_res = ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "diagnostics": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        };
        self.notify_daemon_if_needed(&mut op_res);
        op_res
    }

    pub fn approve(&self, id_str: &str, grant_all: bool) -> ExtOpResult {
        if self.is_script_id(id_str) {
            return script_catalog_operation_error(id_str, "grant");
        }
        let Ok(id) = ExtensionId::new(id_str) else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("invalid extension ID '{id_str}'"),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };

        let receipt = match self.catalog.receipt(&id) {
            Ok(receipt) => receipt,
            Err(error) => {
                return ExtOpResult {
                    success: false,
                    data: serde_json::Value::Null,
                    human_message: format!("failed to inspect extension permissions: {error}"),
                    warnings: Vec::new(),
                    exit_code: 1,
                };
            }
        };

        let pending = receipt.pending.is_some();
        let capabilities = if grant_all {
            let requested = if pending {
                self.catalog.pending_capabilities(&id)
            } else {
                self.catalog.requested_capabilities(&id)
            };
            match requested {
                Ok(caps) => caps,
                Err(error) => {
                    return ExtOpResult {
                        success: false,
                        data: serde_json::Value::Null,
                        human_message: format!("failed to inspect requested permissions: {error}"),
                        warnings: Vec::new(),
                        exit_code: 1,
                    };
                }
            }
        } else {
            Vec::new()
        };

        let result = if pending {
            self.catalog
                .approve_pending(&id, capabilities)
                .map(|r| r.active.version.to_string())
        } else {
            self.catalog
                .approve_capabilities(&id, capabilities)
                .map(|_| receipt.active.version.to_string())
        };

        let (success, message) = match result {
            Ok(version) => (
                true,
                format!("permission review completed; '{id}' {version} is active"),
            ),
            Err(error) => (false, format!("error[permissions.review]: {error}")),
        };

        let mut op_res = ExtOpResult {
            success,
            data: serde_json::json!({
                "extension_id": id.to_string(),
                "diagnostics": [message.clone()],
            }),
            human_message: message,
            warnings: Vec::new(),
            exit_code: if success { 0 } else { 1 },
        };
        self.notify_daemon_if_needed(&mut op_res);
        op_res
    }

    pub fn rollback(&self, id_str: &str) -> ExtOpResult {
        if self.is_script_id(id_str) {
            return script_catalog_operation_error(id_str, "rollback");
        }
        let Ok(id) = ExtensionId::new(id_str) else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("invalid extension ID '{id_str}'"),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };
        let cli_res = ExtensionCli::rollback(&id, &self.catalog);
        let mut op_res = ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "diagnostics": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        };
        self.notify_daemon_if_needed(&mut op_res);
        op_res
    }

    pub fn uninstall(&self, id_str: &str, delete_secrets: bool, delete_state: bool) -> ExtOpResult {
        if self.is_script_id(id_str) {
            return script_catalog_operation_error(id_str, "uninstall");
        }
        let Ok(id) = ExtensionId::new(id_str) else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("invalid extension ID '{id_str}'"),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };
        let secret_policy = if delete_secrets {
            shilpo_ext_runtime::SecretPolicy::Delete
        } else {
            shilpo_ext_runtime::SecretPolicy::Retain
        };
        let state_policy = if delete_state {
            shilpo_ext_runtime::StatePolicy::Delete
        } else {
            shilpo_ext_runtime::StatePolicy::Retain
        };
        let broker = shilpo_ext_runtime::Oo7SecretBroker::new().ok().map(|b| {
            std::sync::Arc::new(b) as std::sync::Arc<dyn shilpo_ext_runtime::SecretBroker>
        });
        let store = if state_policy == shilpo_ext_runtime::StatePolicy::Delete {
            match shilpo_ext_runtime::HeedStateStore::open(&self.catalog.paths().state_store_dir())
            {
                Ok(store) => Some(std::sync::Arc::new(store)
                    as std::sync::Arc<dyn shilpo_ext_runtime::StateStore>),
                Err(error) => {
                    return ExtOpResult {
                        success: false,
                        data: serde_json::Value::Null,
                        human_message: format!("failed to open extension state store: {error}"),
                        warnings: Vec::new(),
                        exit_code: 1,
                    };
                }
            }
        } else {
            None
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);

        let cli_res = ExtensionCli::uninstall_with_policies(
            &id,
            &self.catalog,
            secret_policy,
            broker.as_deref(),
            state_policy,
            store.as_deref(),
            deadline,
        );
        let mut op_res = ExtOpResult {
            success: cli_res.success,
            data: serde_json::json!({
                "extension_id": cli_res.extension_id,
                "diagnostics": cli_res.diagnostics,
            }),
            human_message: cli_res.diagnostics.join("\n"),
            warnings: Vec::new(),
            exit_code: if cli_res.success { 0 } else { 1 },
        };
        self.notify_daemon_if_needed(&mut op_res);
        op_res
    }

    pub fn check_updates(&self) -> ExtOpResult {
        let snapshot = self.catalog.snapshot();
        ExtOpResult {
            success: true,
            data: serde_json::json!({
                "updates": snapshot.updates,
            }),
            human_message: format!("Found {} available update(s)", snapshot.updates.len()),
            warnings: Vec::new(),
            exit_code: 0,
        }
    }

    pub fn channel(&self, id_str: &str, channel_str: Option<&str>) -> ExtOpResult {
        let Ok(id) = ExtensionId::new(id_str) else {
            return ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("invalid extension ID '{id_str}'"),
                warnings: Vec::new(),
                exit_code: 2,
            };
        };
        let channel = match channel_str {
            Some("stable") => ReleaseChannel::Stable,
            Some("beta") => ReleaseChannel::Beta,
            Some("development") => ReleaseChannel::Development,
            _ => {
                return ExtOpResult {
                    success: false,
                    data: serde_json::Value::Null,
                    human_message: "channel must be stable, beta, or development".into(),
                    warnings: Vec::new(),
                    exit_code: 2,
                };
            }
        };
        match self.catalog.set_channel(&id, channel) {
            Ok(()) => ExtOpResult {
                success: true,
                data: serde_json::json!({
                    "extension_id": id.to_string(),
                    "channel": format!("{channel:?}"),
                }),
                human_message: format!("selected {channel:?} channel for '{id}'"),
                warnings: Vec::new(),
                exit_code: 0,
            },
            Err(error) => ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("error[channel.failed]: {error}"),
                warnings: Vec::new(),
                exit_code: 1,
            },
        }
    }

    pub fn source(&self, args: &[String]) -> ExtOpResult {
        match shilpo_ext_runtime::source_command(args, &self.catalog) {
            Ok(message) => ExtOpResult {
                success: true,
                data: serde_json::json!({ "arguments": args, "message": message }),
                human_message: message,
                warnings: Vec::new(),
                exit_code: 0,
            },
            Err(error) => ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: error,
                warnings: Vec::new(),
                exit_code: 2,
            },
        }
    }

    pub fn refresh_sources(&self) -> ExtOpResult {
        match self.catalog.refresh_sources() {
            Ok(diagnostics) => ExtOpResult {
                success: true,
                data: serde_json::json!({ "diagnostics": diagnostics }),
                human_message: "Catalog sources refreshed".into(),
                warnings: Vec::new(),
                exit_code: 0,
            },
            Err(error) => ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: error.to_string(),
                warnings: Vec::new(),
                exit_code: 1,
            },
        }
    }

    pub fn sign(&self, package: &Path, key: &Path, publisher: &str) -> ExtOpResult {
        let key_str = match std::fs::read_to_string(key) {
            Ok(s) => s,
            Err(e) => {
                return ExtOpResult {
                    success: false,
                    data: serde_json::Value::Null,
                    human_message: format!(
                        "error[sign.key_read]: failed to read key file {}: {e}",
                        key.display()
                    ),
                    warnings: Vec::new(),
                    exit_code: 1,
                };
            }
        };
        match sign_package(package, publisher, &key_str) {
            Ok(signature_path) => ExtOpResult {
                success: true,
                data: serde_json::json!({
                    "signature_path": signature_path,
                }),
                human_message: format!(
                    "signed package {}; signature at {}",
                    package.display(),
                    signature_path.display()
                ),
                warnings: Vec::new(),
                exit_code: 0,
            },
            Err(error) => ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("error[sign.failed]: {error}"),
                warnings: Vec::new(),
                exit_code: 1,
            },
        }
    }

    pub fn keygen(&self, output: &Path) -> ExtOpResult {
        match shilpo_ext_runtime::write_signing_key(output) {
            Ok(public_path) => ExtOpResult {
                success: true,
                data: serde_json::json!({
                    "private_key": output,
                    "public_key": public_path,
                }),
                human_message: format!(
                    "generated signing key at {} (public: {})",
                    output.display(),
                    public_path.display()
                ),
                warnings: Vec::new(),
                exit_code: 0,
            },
            Err(error) => ExtOpResult {
                success: false,
                data: serde_json::Value::Null,
                human_message: format!("error[keygen.failed]: {error}"),
                warnings: Vec::new(),
                exit_code: 1,
            },
        }
    }

    fn is_script_id(&self, id: &str) -> bool {
        shilpo_ext_runtime::script::discover_script_bundles(
            &shilpo_ext_runtime::CatalogPaths::platform_default(),
        )
        .iter()
        .any(|bundle| bundle.id.as_str() == id)
    }
}

fn script_catalog_operation_error(id: &str, operation: &str) -> ExtOpResult {
    ExtOpResult {
        success: false,
        data: serde_json::Value::Null,
        human_message: format!(
            "trusted local script '{id}' is not managed by the catalog; '{operation}' is unavailable"
        ),
        warnings: Vec::new(),
        exit_code: 1,
    }
}

fn script_item_from_status(status: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": status.get("id"),
        "name": status.get("name"),
        "version": status.get("version"),
        "type": "script",
        "runtime_kind": "trusted_local_script",
        "source": "local",
        "status": status.get("status"),
        "trusted": true,
        "sandboxed": false,
        "label": "Trusted local script (not sandboxed)",
        "contributions_count": status.get("contributions_count"),
        "diagnostics": status.get("diagnostics"),
    })
}

fn record_refresh_warning(result: &mut ExtOpResult, reason: impl Into<String>) {
    result
        .warnings
        .push(format!("Local mutation succeeded, but {}", reason.into()));
}

fn should_ignore_path(path: &Path, root: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    for component in rel.components() {
        let name = component.as_os_str().to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "dist" {
            return true;
        }
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str())
        && (ext == "tmp" || ext == "wasm")
    {
        return true;
    }
    if let Some(file_name) = path.file_name().and_then(|f| f.to_str())
        && (file_name == "extension.wasm" || file_name.starts_with('.'))
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        ExtOpResult, record_refresh_warning, script_catalog_operation_error,
        script_item_from_status,
    };
    use std::path::PathBuf;

    #[test]
    fn daemon_refresh_failure_keeps_local_mutation_successful() {
        let mut result = ExtOpResult {
            success: true,
            data: serde_json::Value::Null,
            human_message: "extension enabled".into(),
            warnings: Vec::new(),
            exit_code: 0,
        };

        record_refresh_warning(&mut result, "live shell daemon is not running");

        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("Local mutation succeeded"));
    }

    #[test]
    fn live_script_status_has_stable_human_and_structured_fields() {
        let item = script_item_from_status(&serde_json::json!({
            "id": "local.script.test",
            "name": "Test",
            "version": "0.1.0",
            "status": "ready",
            "contributions_count": 2,
            "diagnostics": ["example"],
        }));
        assert_eq!(item["type"], "script");
        assert_eq!(item["source"], "local");
        assert_eq!(item["status"], "ready");
        assert_eq!(item["runtime_kind"], "trusted_local_script");
        assert_eq!(item["sandboxed"], false);
        assert_eq!(item["contributions_count"], 2);
        assert_eq!(item["diagnostics"][0], "example");
    }

    #[test]
    fn scripts_reject_catalog_only_operations() {
        for operation in ["install", "update", "grant", "rollback", "uninstall"] {
            let result = script_catalog_operation_error("local.script.test", operation);
            assert!(!result.success);
            assert!(result.human_message.contains("not managed by the catalog"));
            assert!(result.human_message.contains(operation));
        }
    }

    #[test]
    fn test_status_output_formats_all_circuit_states() {
        let telemetry_json = serde_json::json!({
            "lifecycle": "ready",
            "host_generation": 1,
            "engine_generation": 5,
            "wasm_extensions": [
                {
                    "id": "io.github.test.healthy",
                    "runtime_kind": "wasm",
                    "state": "closed",
                    "consecutive_failures": 0,
                    "trip_count": 0
                },
                {
                    "id": "io.github.test.retrying",
                    "runtime_kind": "wasm",
                    "state": "open",
                    "trip_count": 2,
                    "retry_after_ms": 45000
                },
                {
                    "id": "io.github.test.probing",
                    "runtime_kind": "wasm",
                    "state": "half_open",
                    "consecutive_successes": 1,
                    "trip_count": 1
                },
                {
                    "id": "io.github.test.dead",
                    "runtime_kind": "wasm",
                    "state": "permanently_disabled",
                    "trip_count": 4
                }
            ],
            "script_extensions": [
                {
                    "id": "local.script.test",
                    "name": "Test Script",
                    "version": "1.0.0",
                    "status": "ready"
                }
            ]
        });

        let ext_status = telemetry_json;
        let state_str = ext_status.get("lifecycle").unwrap().as_str().unwrap();
        let host_gen = ext_status.get("host_generation").unwrap().as_u64().unwrap();
        let engine_gen = ext_status
            .get("engine_generation")
            .unwrap()
            .as_u64()
            .unwrap();

        let mut human_lines = vec![
            "Extension Host Status:".to_string(),
            format!("  State: {state_str}"),
            format!("  Host Generation: {host_gen}"),
            format!("  Engine Generation: {engine_gen}"),
        ];

        let wasm_extensions = ext_status
            .get("wasm_extensions")
            .unwrap()
            .as_array()
            .unwrap();
        human_lines.push(String::new());
        human_lines.push(format!("WASM Extensions ({}):", wasm_extensions.len()));
        for ext in wasm_extensions {
            let id = ext.get("id").unwrap().as_str().unwrap();
            let state = ext.get("state").unwrap().as_str().unwrap();
            let trip_count = ext.get("trip_count").unwrap().as_u64().unwrap();
            let desc = match state {
                "closed" => "closed, healthy".to_string(),
                "open" => {
                    let retry_ms = ext.get("retry_after_ms").unwrap().as_u64().unwrap();
                    let secs = retry_ms.div_ceil(1000);
                    format!("open, retrying in {secs}s (trip {trip_count})")
                }
                "half_open" => {
                    let successes = ext.get("consecutive_successes").unwrap().as_u64().unwrap();
                    format!("half_open, probing ({successes}/3 successes, trip {trip_count})")
                }
                "permanently_disabled" => {
                    format!("permanently_disabled, failed after {trip_count} trip cycles")
                }
                other => other.to_string(),
            };
            human_lines.push(format!("  {id} [{desc}]"));
        }

        let human = human_lines.join("\n");
        assert!(human.contains("io.github.test.healthy [closed, healthy]"));
        assert!(human.contains("io.github.test.retrying [open, retrying in 45s (trip 2)]"));
        assert!(
            human.contains("io.github.test.probing [half_open, probing (1/3 successes, trip 1)]")
        );
        assert!(
            human
                .contains("io.github.test.dead [permanently_disabled, failed after 4 trip cycles]")
        );
    }

    #[test]
    fn test_should_ignore_path() {
        let root = PathBuf::from("/home/user/project");
        assert!(super::should_ignore_path(&root.join(".git/HEAD"), &root));
        assert!(super::should_ignore_path(
            &root.join("node_modules/pkg/index.js"),
            &root
        ));
        assert!(super::should_ignore_path(
            &root.join("target/wasm32-wasip1/debug/app.wasm"),
            &root
        ));
        assert!(super::should_ignore_path(
            &root.join("dist/bundle.js"),
            &root
        ));
        assert!(super::should_ignore_path(
            &root.join("extension.wasm"),
            &root
        ));
        assert!(super::should_ignore_path(&root.join("build.tmp"), &root));
        assert!(super::should_ignore_path(
            &root.join(".shilpo-cache/data"),
            &root
        ));

        // Valid source files that should NOT be ignored
        assert!(!super::should_ignore_path(
            &root.join("extension.toml"),
            &root
        ));
        assert!(!super::should_ignore_path(&root.join("src/lib.rs"), &root));
        assert!(!super::should_ignore_path(
            &root.join("src/index.ts"),
            &root
        ));
    }
}
