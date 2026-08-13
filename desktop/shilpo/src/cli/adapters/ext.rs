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
                let human = format!(
                    "Extension Host Status:\n  State: {}\n  Host Generation: {}\n  Engine Generation: {}",
                    state_str, host_gen, engine_gen
                );
                ExtOpResult {
                    success: true,
                    data: ext_status,
                    human_message: human,
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

    pub fn dev(&self, path: &Path) -> ExtOpResult {
        let cli_res = ExtensionCli::dev(path, &self.state_dir);
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

    pub fn reload(&self, id_str: Option<&str>) -> ExtOpResult {
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
        let cli_res = ExtensionCli::reload(&id, &self.state_dir);
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
        let catalog_paths = shilpo_ext_runtime::CatalogPaths::platform_default();
        let script_bundles = shilpo_ext_runtime::script::discover_script_bundles(&catalog_paths);
        let script_items: Vec<serde_json::Value> = script_bundles
            .iter()
            .map(|b| {
                serde_json::json!({
                    "id": b.id,
                    "name": b.name,
                    "version": b.version,
                    "path": b.path,
                    "mode": b.mode,
                    "runtime_kind": "trusted_local_script",
                    "trusted": true,
                    "sandboxed": false,
                    "label": "Trusted local script (not sandboxed)",
                    "contributions_count": b.contributions_count,
                    "diagnostics": b.diagnostics,
                })
            })
            .collect();

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
            script_bundles.len()
        ));
        for b in &script_bundles {
            human_lines.push(format!(
                "  {} (v{}) [script, local] - {} contributions (Trusted local script (not sandboxed))",
                b.id, b.version, b.contributions_count
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
}

fn record_refresh_warning(result: &mut ExtOpResult, reason: impl Into<String>) {
    result
        .warnings
        .push(format!("Local mutation succeeded, but {}", reason.into()));
}

#[cfg(test)]
mod tests {
    use super::{ExtOpResult, record_refresh_warning};

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
}
