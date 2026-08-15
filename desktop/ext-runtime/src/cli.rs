use crate::adapter::{ExtensionRuntime, RuntimeBudget};
use crate::catalog::{
    ExtensionCatalog, RegistrySource, ReleaseChannel, UpdateState, generate_signing_key,
    sign_package,
};
use crate::wasm::{WasmModule, WasmRuntime};
use flate2::Compression;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use shilpo_ext_api::{ExtensionEvent, ExtensionId, ExtensionManifest, ViewLimits};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tar::Builder;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionCliResult {
    pub success: bool,
    pub extension_id: Option<String>,
    pub artifact: Option<PathBuf>,
    pub diagnostics: Vec<String>,
}

impl ExtensionCliResult {
    fn failure(extension_id: Option<String>, diagnostics: Vec<String>) -> Self {
        Self {
            success: false,
            extension_id,
            artifact: None,
            diagnostics,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevelopmentRegistration {
    pub id: ExtensionId,
    pub path: PathBuf,
    pub generation: u64,
    pub updated_at_unix_seconds: u64,
}

struct CheckedExtension {
    manifest: ExtensionManifest,
    files: Vec<PathBuf>,
    diagnostics: Vec<String>,
}

pub struct ExtensionCli;

impl ExtensionCli {
    pub fn build(dir: &Path, release: bool) -> ExtensionCliResult {
        crate::build::build_extension(dir, release, &crate::build::OsProcessRunner)
    }

    pub fn build_with_timeout(
        dir: &Path,
        release: bool,
        timeout: std::time::Duration,
    ) -> ExtensionCliResult {
        crate::build::build_extension_with_timeout(
            dir,
            release,
            &crate::build::OsProcessRunner,
            timeout,
        )
    }

    pub fn build_with_runner(
        dir: &Path,
        release: bool,
        runner: &dyn crate::build::ProcessRunner,
    ) -> ExtensionCliResult {
        crate::build::build_extension(dir, release, runner)
    }

    pub fn lint(dir: &Path, options: crate::lint::LintOptions) -> crate::lint::LintReport {
        crate::lint::inspect_extension_with_timeout(dir, options.deny_warnings, options.timeout)
    }

    pub fn check_report(dir: &Path) -> crate::lint::LintReport {
        crate::lint::inspect_extension(dir, crate::lint::InspectionPolicy::Check)
    }

    pub fn check(dir: &Path) -> ExtensionCliResult {
        match inspect_extension(dir) {
            Ok(checked) => ExtensionCliResult {
                success: true,
                extension_id: Some(checked.manifest.id.to_string()),
                artifact: None,
                diagnostics: checked.diagnostics,
            },
            Err((extension_id, diagnostics)) => {
                ExtensionCliResult::failure(extension_id, diagnostics)
            }
        }
    }

    pub fn pack(dir: &Path, output_dir: &Path) -> ExtensionCliResult {
        let checked = match inspect_extension(dir) {
            Ok(checked) => checked,
            Err((extension_id, diagnostics)) => {
                return ExtensionCliResult::failure(extension_id, diagnostics);
            }
        };

        let extension_id = checked.manifest.id.to_string();
        let archive_name = format!(
            "{}-{}.shilpo-ext",
            checked.manifest.id, checked.manifest.version
        );
        let target_path = output_dir.join(archive_name);
        let temporary_path =
            target_path.with_extension(format!("shilpo-ext.{}.tmp", std::process::id()));
        let mut diagnostics = checked.diagnostics;

        if let Err(error) = fs::create_dir_all(output_dir) {
            diagnostics.push(format!(
                "error[package.output]: failed to create {}: {error}",
                output_dir.display()
            ));
            return ExtensionCliResult::failure(Some(extension_id), diagnostics);
        }

        let result = (|| -> Result<(), String> {
            let output = File::create(&temporary_path)
                .map_err(|error| format!("failed to create package: {error}"))?;
            let encoder = GzEncoder::new(output, Compression::best());
            let mut archive = Builder::new(encoder);
            archive.mode(tar::HeaderMode::Deterministic);
            archive.follow_symlinks(false);

            for relative in &checked.files {
                let source = dir.join(relative);
                archive
                    .append_path_with_name(&source, relative)
                    .map_err(|error| format!("failed to add {}: {error}", relative.display()))?;
            }
            let encoder = archive
                .into_inner()
                .map_err(|error| format!("failed to finish package archive: {error}"))?;
            encoder
                .finish()
                .map_err(|error| format!("failed to finish package compression: {error}"))?;
            fs::rename(&temporary_path, &target_path)
                .map_err(|error| format!("failed to activate package archive: {error}"))?;
            Ok(())
        })();

        if let Err(error) = result {
            let _ = fs::remove_file(&temporary_path);
            diagnostics.push(format!("error[package.write]: {error}"));
            return ExtensionCliResult::failure(Some(extension_id), diagnostics);
        }

        diagnostics.push(format!("info[package.created]: {}", target_path.display()));
        ExtensionCliResult {
            success: true,
            extension_id: Some(extension_id),
            artifact: Some(target_path),
            diagnostics,
        }
    }

    pub fn dev(dir: &Path, state_dir: &Path) -> ExtensionCliResult {
        let checked = match inspect_extension(dir) {
            Ok(checked) => checked,
            Err((extension_id, diagnostics)) => {
                return ExtensionCliResult::failure(extension_id, diagnostics);
            }
        };
        let id = checked.manifest.id.clone();
        let mut diagnostics = checked.diagnostics;
        if let Err(error) = probe_runtime(dir, &checked.manifest) {
            diagnostics.push(format!("error[dev.runtime]: {error}"));
            return ExtensionCliResult::failure(Some(id.to_string()), diagnostics);
        }
        let path = match dir.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                return ExtensionCliResult::failure(
                    Some(id.to_string()),
                    vec![format!(
                        "error[dev.path]: failed to resolve {}: {error}",
                        dir.display()
                    )],
                );
            }
        };
        let registration = DevelopmentRegistration {
            id: id.clone(),
            path,
            generation: 1,
            updated_at_unix_seconds: unix_timestamp(),
        };
        if let Err(error) = write_registration(state_dir, &registration) {
            diagnostics.push(format!("error[dev.register]: {error}"));
            return ExtensionCliResult::failure(Some(id.to_string()), diagnostics);
        }
        let message = format!(
            "registered development override generation {} from {}",
            registration.generation,
            registration.path.display()
        );
        let _ = append_log(state_dir, &id, &message);
        diagnostics.push(format!("info[dev.registered]: {message}"));
        ExtensionCliResult {
            success: true,
            extension_id: Some(id.to_string()),
            artifact: None,
            diagnostics,
        }
    }

    pub fn reload(id: &ExtensionId, state_dir: &Path) -> ExtensionCliResult {
        let mut registration = match read_registration(state_dir, id) {
            Ok(registration) => registration,
            Err(error) => {
                return ExtensionCliResult::failure(
                    Some(id.to_string()),
                    vec![format!("error[dev.not-registered]: {error}")],
                );
            }
        };
        let checked = match inspect_extension(&registration.path) {
            Ok(checked) => checked,
            Err((_, diagnostics)) => {
                let _ = append_log(
                    state_dir,
                    id,
                    &format!("reload rejected: {}", diagnostics.join("; ")),
                );
                return ExtensionCliResult::failure(Some(id.to_string()), diagnostics);
            }
        };
        if checked.manifest.id != *id {
            return ExtensionCliResult::failure(
                Some(id.to_string()),
                vec![format!(
                    "error[dev.identity]: development path now declares '{}'",
                    checked.manifest.id
                )],
            );
        }
        let mut diagnostics = checked.diagnostics;
        if let Err(error) = probe_runtime(&registration.path, &checked.manifest) {
            diagnostics.push(format!("error[dev.runtime]: {error}"));
            let _ = append_log(state_dir, id, &format!("reload rejected: {error}"));
            return ExtensionCliResult::failure(Some(id.to_string()), diagnostics);
        }

        registration.generation += 1;
        registration.updated_at_unix_seconds = unix_timestamp();
        if let Err(error) = write_registration(state_dir, &registration) {
            diagnostics.push(format!("error[dev.reload]: {error}"));
            return ExtensionCliResult::failure(Some(id.to_string()), diagnostics);
        }
        let message = format!(
            "validated development override generation {}",
            registration.generation
        );
        let _ = append_log(state_dir, id, &message);
        diagnostics.push(format!("info[dev.reloaded]: {message}"));
        ExtensionCliResult {
            success: true,
            extension_id: Some(id.to_string()),
            artifact: None,
            diagnostics,
        }
    }

    pub fn logs(id: &ExtensionId, state_dir: &Path) -> ExtensionCliResult {
        let path = log_path(state_dir, id);
        match fs::read_to_string(&path) {
            Ok(log) => ExtensionCliResult {
                success: true,
                extension_id: Some(id.to_string()),
                artifact: Some(path),
                diagnostics: log.lines().map(ToOwned::to_owned).collect(),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ExtensionCliResult::failure(
                    Some(id.to_string()),
                    vec!["error[logs.missing]: no development log exists".into()],
                )
            }
            Err(error) => ExtensionCliResult::failure(
                Some(id.to_string()),
                vec![format!("error[logs.read]: {error}")],
            ),
        }
    }

    pub fn list_dev(state_dir: &Path) -> Vec<ExtensionCliResult> {
        development_registrations(state_dir)
            .0
            .into_iter()
            .map(|registration| Self::check(&registration.path))
            .collect()
    }

    pub fn install(package: &Path, catalog: &ExtensionCatalog) -> ExtensionCliResult {
        match catalog.install_local(package) {
            Ok(receipt) => ExtensionCliResult {
                success: true,
                extension_id: Some(receipt.id.to_string()),
                artifact: None,
                diagnostics: vec![format!(
                    "installed '{}' {} disabled; review permissions before enabling",
                    receipt.id, receipt.active.version
                )],
            },
            Err(error) => {
                ExtensionCliResult::failure(None, vec![format!("error[install.failed]: {error}")])
            }
        }
    }

    pub fn install_from_catalog(
        id: &ExtensionId,
        catalog: &ExtensionCatalog,
    ) -> ExtensionCliResult {
        match catalog.install_from_catalog(id) {
            Ok(receipt) => ExtensionCliResult {
                success: true,
                extension_id: Some(receipt.id.to_string()),
                artifact: None,
                diagnostics: vec![if let Some(pending) = &receipt.pending {
                    format!(
                        "staged '{}' {}; permission review is required",
                        receipt.id, pending.version
                    )
                } else {
                    format!("installed '{}' {}", receipt.id, receipt.active.version)
                }],
            },
            Err(error) => ExtensionCliResult::failure(
                Some(id.to_string()),
                vec![format!("error[update.failed]: {error}")],
            ),
        }
    }

    pub fn enable(id: &ExtensionId, catalog: &ExtensionCatalog) -> ExtensionCliResult {
        match catalog.set_enabled(id, true) {
            Ok(()) => ExtensionCliResult {
                success: true,
                extension_id: Some(id.to_string()),
                artifact: None,
                diagnostics: vec![format!("extension '{id}' enabled")],
            },
            Err(error) => ExtensionCliResult::failure(
                Some(id.to_string()),
                vec![format!("error[enable.failed]: {error}")],
            ),
        }
    }

    pub fn disable(id: &ExtensionId, catalog: &ExtensionCatalog) -> ExtensionCliResult {
        match catalog.set_enabled(id, false) {
            Ok(()) => ExtensionCliResult {
                success: true,
                extension_id: Some(id.to_string()),
                artifact: None,
                diagnostics: vec![format!("extension '{id}' disabled")],
            },
            Err(error) => ExtensionCliResult::failure(
                Some(id.to_string()),
                vec![format!("error[disable.failed]: {error}")],
            ),
        }
    }

    pub fn uninstall(id: &ExtensionId, catalog: &ExtensionCatalog) -> ExtensionCliResult {
        Self::uninstall_with_policies(
            id,
            catalog,
            crate::catalog::SecretPolicy::Retain,
            None,
            crate::state::StatePolicy::Retain,
            None,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
    }

    pub fn uninstall_with_policies(
        id: &ExtensionId,
        catalog: &ExtensionCatalog,
        secret_policy: crate::catalog::SecretPolicy,
        broker: Option<&dyn crate::secrets::SecretBroker>,
        state_policy: crate::state::StatePolicy,
        state_store: Option<&dyn crate::state::StateStore>,
        deadline: std::time::Instant,
    ) -> ExtensionCliResult {
        match catalog.uninstall_with_policies(
            id,
            secret_policy,
            broker,
            state_policy,
            state_store,
            deadline,
        ) {
            Ok(()) => ExtensionCliResult {
                success: true,
                extension_id: Some(id.to_string()),
                artifact: None,
                diagnostics: vec![format!("extension '{id}' uninstalled")],
            },
            Err(error) => ExtensionCliResult::failure(
                Some(id.to_string()),
                vec![format!("error[uninstall.failed]: {error}")],
            ),
        }
    }

    pub fn rollback(id: &ExtensionId, catalog: &ExtensionCatalog) -> ExtensionCliResult {
        match catalog.rollback(id) {
            Ok(receipt) => ExtensionCliResult {
                success: true,
                extension_id: Some(id.to_string()),
                artifact: None,
                diagnostics: vec![format!(
                    "rolled '{}' back to {}",
                    receipt.id, receipt.active.version
                )],
            },
            Err(error) => ExtensionCliResult::failure(
                Some(id.to_string()),
                vec![format!("error[rollback.failed]: {error}")],
            ),
        }
    }

    pub fn list(dev_paths: &[PathBuf]) -> Vec<ExtensionCliResult> {
        dev_paths.iter().map(|path| Self::check(path)).collect()
    }
}

#[allow(dead_code)]
fn run_cli(args: &[String]) -> i32 {
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return 2;
    };
    let state_dir = default_extension_state_dir();
    let catalog = ExtensionCatalog::open_default();
    let result = match command {
        "build" => {
            let source = Path::new(args.get(1).map_or(".", String::as_str));
            let release = args.iter().any(|arg| arg == "--release");
            ExtensionCli::build(source, release)
        }
        "check" => ExtensionCli::check(Path::new(args.get(1).map_or(".", String::as_str))),
        "pack" => {
            let source = Path::new(args.get(1).map_or(".", String::as_str));
            let output = args
                .windows(2)
                .find(|pair| pair[0] == "--output")
                .map_or_else(|| source.join("dist"), |pair| PathBuf::from(&pair[1]));
            ExtensionCli::pack(source, &output)
        }
        "dev" => {
            let Some(path) = args.get(1) else {
                eprintln!("shilpo ext dev requires an extension path");
                return 2;
            };
            ExtensionCli::dev(Path::new(path), &state_dir)
        }
        "reload" => {
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            ExtensionCli::reload(&id, &state_dir)
        }
        "logs" => {
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            let follow = args.iter().any(|argument| argument == "--follow");
            let result = ExtensionCli::logs(&id, &state_dir);
            print_result(&result);
            if result.success && follow {
                follow_log(&id, &state_dir);
            }
            return (!result.success) as i32;
        }
        "install" => {
            let Some(target) = args.get(1) else {
                eprintln!("shilpo ext install requires a package path or extension ID");
                return 2;
            };
            if target.starts_with("https://") {
                match catalog.install_url(target, option_value(args, "--hash")) {
                    Ok(receipt) => ExtensionCliResult {
                        success: true,
                        extension_id: Some(receipt.id.to_string()),
                        artifact: None,
                        diagnostics: vec![format!(
                            "installed '{}' {} disabled",
                            receipt.id, receipt.active.version
                        )],
                    },
                    Err(error) => ExtensionCliResult::failure(
                        None,
                        vec![format!("error[install.url]: {error}")],
                    ),
                }
            } else {
                match ExtensionId::new(target.clone()) {
                    Ok(id) if !Path::new(target).exists() => {
                        ExtensionCli::install_from_catalog(&id, &catalog)
                    }
                    _ => ExtensionCli::install(Path::new(target), &catalog),
                }
            }
        }
        "update" => {
            if args.get(1).is_some_and(|argument| argument == "--all") {
                let snapshot = catalog_snapshot(&catalog, &state_dir);
                let available = snapshot
                    .updates
                    .into_iter()
                    .filter(|update| update.state == UpdateState::Available)
                    .map(|update| update.id)
                    .collect::<Vec<_>>();
                if args.iter().any(|argument| argument == "--dry-run") {
                    for id in available {
                        println!("would update {id}");
                    }
                    return 0;
                }
                let results = available
                    .iter()
                    .map(|id| ExtensionCli::install_from_catalog(id, &catalog))
                    .collect::<Vec<_>>();
                let failed = results.iter().any(|result| !result.success);
                for result in results {
                    print_result(&result);
                }
                return failed as i32;
            }
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            ExtensionCli::install_from_catalog(&id, &catalog)
        }
        "channel" => {
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            let channel = match args.get(2).map(String::as_str) {
                Some("stable") => ReleaseChannel::Stable,
                Some("beta") => ReleaseChannel::Beta,
                Some("development") => ReleaseChannel::Development,
                _ => {
                    eprintln!("channel must be stable, beta, or development");
                    return 2;
                }
            };
            match catalog.set_channel(&id, channel) {
                Ok(()) => ExtensionCliResult {
                    success: true,
                    extension_id: Some(id.to_string()),
                    artifact: None,
                    diagnostics: vec![format!("selected {channel:?} channel for '{id}'")],
                },
                Err(error) => ExtensionCliResult::failure(
                    Some(id.to_string()),
                    vec![format!("error[channel.failed]: {error}")],
                ),
            }
        }
        "enable" => {
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            ExtensionCli::enable(&id, &catalog)
        }
        "disable" => {
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            ExtensionCli::disable(&id, &catalog)
        }
        "uninstall" => {
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            let delete_secrets = args
                .iter()
                .any(|arg| arg == "--delete-secrets" || arg == "--purge-secrets");
            let delete_state = args
                .iter()
                .any(|arg| arg == "--delete-state" || arg == "--purge-state");
            let secret_policy = if delete_secrets {
                crate::catalog::SecretPolicy::Delete
            } else {
                crate::catalog::SecretPolicy::Retain
            };
            let state_policy = if delete_state {
                crate::state::StatePolicy::Delete
            } else {
                crate::state::StatePolicy::Retain
            };
            let broker = crate::secrets::Oo7SecretBroker::new().ok().map(|b| {
                std::sync::Arc::new(b) as std::sync::Arc<dyn crate::secrets::SecretBroker>
            });
            let store =
                if state_policy == crate::state::StatePolicy::Delete {
                    match crate::state::HeedStateStore::open(&catalog.paths().state_store_dir()) {
                        Ok(store) => Some(std::sync::Arc::new(store)
                            as std::sync::Arc<dyn crate::state::StateStore>),
                        Err(error) => {
                            eprintln!("error[uninstall.state_store]: {error}");
                            return 1;
                        }
                    }
                } else {
                    None
                };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);

            ExtensionCli::uninstall_with_policies(
                &id,
                &catalog,
                secret_policy,
                broker.as_deref(),
                state_policy,
                store.as_deref(),
                deadline,
            )
        }
        "rollback" => {
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            ExtensionCli::rollback(&id, &catalog)
        }
        "approve" => {
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            let receipt = match catalog.receipt(&id) {
                Ok(receipt) => receipt,
                Err(error) => {
                    eprintln!("failed to inspect extension permissions: {error}");
                    return 1;
                }
            };
            let pending = receipt.pending.is_some();
            let capabilities = if args.iter().any(|argument| argument == "--grant-all") {
                let requested = if pending {
                    catalog.pending_capabilities(&id)
                } else {
                    catalog.requested_capabilities(&id)
                };
                match requested {
                    Ok(capabilities) => capabilities,
                    Err(error) => {
                        eprintln!("failed to inspect requested permissions: {error}");
                        return 1;
                    }
                }
            } else {
                Vec::new()
            };
            let result = if pending {
                catalog
                    .approve_pending(&id, capabilities)
                    .map(|receipt| receipt.active.version.to_string())
            } else {
                catalog
                    .approve_capabilities(&id, capabilities)
                    .map(|_| receipt.active.version.to_string())
            };
            match result {
                Ok(version) => ExtensionCliResult {
                    success: true,
                    extension_id: Some(id.to_string()),
                    artifact: None,
                    diagnostics: vec![format!(
                        "permission review completed; '{}' {} is active",
                        id, version
                    )],
                },
                Err(error) => ExtensionCliResult::failure(
                    Some(id.to_string()),
                    vec![format!("error[permissions.review]: {error}")],
                ),
            }
        }
        "keygen" => {
            let Some(path) = args.get(1).map(PathBuf::from) else {
                eprintln!("shilpo ext keygen requires an output path");
                return 2;
            };
            match write_signing_key(&path) {
                Ok(public_path) => ExtensionCliResult {
                    success: true,
                    extension_id: None,
                    artifact: Some(path),
                    diagnostics: vec![format!(
                        "created Ed25519 signing key; public key: {}",
                        public_path.display()
                    )],
                },
                Err(error) => ExtensionCliResult::failure(
                    None,
                    vec![format!("error[keygen.failed]: {error}")],
                ),
            }
        }
        "sign" => {
            let Some(package) = args.get(1).map(PathBuf::from) else {
                eprintln!("shilpo ext sign requires a package path");
                return 2;
            };
            let Some(key_path) = option_value(args, "--key").map(PathBuf::from) else {
                eprintln!("shilpo ext sign requires --key <private-key>");
                return 2;
            };
            let Some(publisher) = option_value(args, "--publisher") else {
                eprintln!("shilpo ext sign requires --publisher <name>");
                return 2;
            };
            match fs::read_to_string(&key_path)
                .map_err(|error| error.to_string())
                .and_then(|key| {
                    sign_package(&package, publisher, &key).map_err(|error| error.to_string())
                }) {
                Ok(sidecar) => ExtensionCliResult {
                    success: true,
                    extension_id: None,
                    artifact: Some(sidecar.clone()),
                    diagnostics: vec![format!("signed package; signature: {}", sidecar.display())],
                },
                Err(error) => {
                    ExtensionCliResult::failure(None, vec![format!("error[sign.failed]: {error}")])
                }
            }
        }
        "refresh-sources" => match catalog.refresh_sources() {
            Ok(diagnostics) => ExtensionCliResult {
                success: true,
                extension_id: None,
                artifact: None,
                diagnostics,
            },
            Err(error) => {
                ExtensionCliResult::failure(None, vec![format!("error[source.refresh]: {error}")])
            }
        },
        "source" => {
            let Some(action) = args.get(1).map(String::as_str) else {
                eprintln!("shilpo ext source requires add, remove, or sync");
                return 2;
            };
            match run_source_command(action, args, &catalog) {
                Ok(message) => ExtensionCliResult {
                    success: true,
                    extension_id: None,
                    artifact: None,
                    diagnostics: vec![message],
                },
                Err(error) => ExtensionCliResult::failure(
                    None,
                    vec![format!("error[source.{action}]: {error}")],
                ),
            }
        }
        "search" => {
            let query = args.get(1).map_or("", String::as_str).to_lowercase();
            let snapshot = catalog_snapshot(&catalog, &state_dir);
            for entry in snapshot.discover.iter().filter(|entry| {
                query.is_empty()
                    || entry.release.name.to_lowercase().contains(&query)
                    || entry.release.id.as_str().contains(&query)
                    || entry
                        .release
                        .description
                        .as_deref()
                        .is_some_and(|description| description.to_lowercase().contains(&query))
            }) {
                println!(
                    "{} {} — {} ({})",
                    entry.release.id, entry.release.version, entry.release.name, entry.trust
                );
            }
            for diagnostic in snapshot.diagnostics {
                eprintln!("{diagnostic}");
            }
            return 0;
        }
        "info" => {
            let Some(id) = parse_id(args.get(1)) else {
                return 2;
            };
            let snapshot = catalog_snapshot(&catalog, &state_dir);
            if let Some(installed) = snapshot
                .installed
                .iter()
                .find(|extension| extension.receipt.id == id)
            {
                println!(
                    "{} {} — {} [{}; {}]",
                    installed.manifest.name,
                    installed.receipt.active.version,
                    installed.receipt.id,
                    installed.receipt.active.trust,
                    if installed.grants.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
                println!(
                    "Source: {}\nCapabilities: {} requested, {} granted",
                    installed.receipt.active.source,
                    installed.manifest.capabilities.len(),
                    installed.grants.granted_capabilities.len()
                );
                return 0;
            }
            if let Some(entry) = snapshot
                .discover
                .iter()
                .find(|entry| entry.release.id == id)
            {
                println!(
                    "{} {} — {} [{}; source {}]",
                    entry.release.name,
                    entry.release.version,
                    entry.release.id,
                    entry.trust,
                    entry.source.id
                );
                return 0;
            }
            eprintln!("extension '{id}' was not found");
            return 1;
        }
        "check-updates" => {
            let snapshot = catalog_snapshot(&catalog, &state_dir);
            for update in &snapshot.updates {
                println!(
                    "{} {}: {:?}",
                    update.id, update.installed_version, update.state
                );
            }
            for diagnostic in snapshot.diagnostics {
                eprintln!("{diagnostic}");
            }
            return 0;
        }
        "list" => {
            let snapshot = catalog_snapshot(&catalog, &state_dir);
            for extension in &snapshot.installed {
                println!(
                    "{} {} [{}; {}]",
                    extension.receipt.id,
                    extension.receipt.active.version,
                    extension.receipt.active.trust,
                    if extension.grants.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
            }
            let results = ExtensionCli::list_dev(&state_dir);
            if snapshot.installed.is_empty() && results.is_empty() {
                println!("No extensions installed or registered for development.");
                return 0;
            }
            let failed = results.iter().any(|result| !result.success);
            for result in results {
                print_result(&result);
            }
            return failed as i32;
        }
        _ => {
            eprintln!("unknown extension command '{command}'");
            print_usage();
            return 2;
        }
    };

    print_result(&result);
    (!result.success) as i32
}

fn catalog_snapshot(
    catalog: &ExtensionCatalog,
    state_dir: &Path,
) -> crate::catalog::ExtensionCatalogSnapshot {
    let ids = development_registrations(state_dir)
        .0
        .into_iter()
        .map(|registration| registration.id);
    catalog.snapshot_with_development(ids)
}

fn inspect_extension(dir: &Path) -> Result<CheckedExtension, (Option<String>, Vec<String>)> {
    match crate::lint::inspect_extension_checked(dir) {
        Ok(checked) => {
            let diagnostics = checked
                .report
                .diagnostics
                .into_iter()
                .map(|d| format!("{}[{}]: {}", d.severity, d.rule_id, d.message))
                .collect();
            Ok(CheckedExtension {
                manifest: checked.manifest,
                files: checked.files,
                diagnostics,
            })
        }
        Err(err) => Err(err),
    }
}

fn option_value<'a>(args: &'a [String], option: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].as_str())
}

pub fn write_signing_key(path: &Path) -> Result<PathBuf, String> {
    let (private_key, public_key) = generate_signing_key().map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut private_file = options.open(path).map_err(|error| error.to_string())?;
    private_file
        .write_all(private_key.as_bytes())
        .map_err(|error| error.to_string())?;
    private_file.sync_all().map_err(|error| error.to_string())?;
    let public_path = PathBuf::from(format!("{}.pub", path.display()));
    fs::write(&public_path, public_key).map_err(|error| error.to_string())?;
    Ok(public_path)
}

fn run_source_command(
    action: &str,
    args: &[String],
    catalog: &ExtensionCatalog,
) -> Result<String, String> {
    match action {
        "add" => {
            let id = args
                .get(2)
                .ok_or_else(|| "source add requires <id> <name> <url> <root-key>".to_owned())?;
            let name = args
                .get(3)
                .ok_or_else(|| "source add requires <id> <name> <url> <root-key>".to_owned())?;
            let index_url = args
                .get(4)
                .ok_or_else(|| "source add requires <id> <name> <url> <root-key>".to_owned())?;
            let key_argument = args
                .get(5)
                .ok_or_else(|| "source add requires <id> <name> <url> <root-key>".to_owned())?;
            let root_public_key = fs::read_to_string(key_argument)
                .unwrap_or_else(|_| key_argument.clone())
                .trim()
                .to_owned();
            catalog
                .add_source(RegistrySource {
                    id: id.clone(),
                    name: name.clone(),
                    index_url: index_url.clone(),
                    root_public_key,
                    official: false,
                    enabled: true,
                })
                .map_err(|error| error.to_string())?;
            Ok(format!("added extension source '{id}'"))
        }
        "remove" => {
            let id = args
                .get(2)
                .ok_or_else(|| "source remove requires <id>".to_owned())?;
            catalog
                .remove_source(id)
                .map_err(|error| error.to_string())?;
            Ok(format!("removed extension source '{id}'"))
        }
        "sync" => {
            let id = args
                .get(2)
                .ok_or_else(|| "source sync requires <id> <signed-index.json>".to_owned())?;
            let path = args
                .get(3)
                .ok_or_else(|| "source sync requires <id> <signed-index.json>".to_owned())?;
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            catalog
                .store_index_bytes(id, &bytes)
                .map_err(|error| error.to_string())?;
            Ok(format!("verified and cached source '{id}'"))
        }
        _ => Err(format!("unknown source action '{action}'")),
    }
}

pub fn source_command(args: &[String], catalog: &ExtensionCatalog) -> Result<String, String> {
    let action = args
        .first()
        .ok_or_else(|| "source requires add, remove, or sync".to_owned())?;
    let mut legacy_args = Vec::with_capacity(args.len() + 1);
    legacy_args.push("source".to_owned());
    legacy_args.extend(args.iter().cloned());
    run_source_command(action, &legacy_args, catalog)
}

pub(crate) fn probe_runtime(dir: &Path, manifest: &ExtensionManifest) -> Result<(), String> {
    let Some(library) = &manifest.library else {
        return Ok(());
    };
    let module =
        WasmModule::from_file(&dir.join(&library.path)).map_err(|error| error.to_string())?;
    let mut runtime = WasmRuntime::new().map_err(|error| error.to_string())?;
    runtime
        .load(&manifest.id, module, RuntimeBudget::default())
        .map_err(|error| error.to_string())?;
    runtime
        .dispatch(
            &manifest.id,
            &ExtensionEvent::ShellStarted,
            RuntimeBudget::default(),
        )
        .map_err(|error| error.to_string())?;
    for contribution_id in manifest
        .contributions
        .bar_widgets
        .iter()
        .map(|contribution| contribution.id.as_str())
        .chain(
            manifest
                .contributions
                .desktop_widgets
                .iter()
                .map(|contribution| contribution.id.as_str()),
        )
    {
        if let Some(view) = runtime
            .view(&manifest.id, contribution_id, RuntimeBudget::default())
            .map_err(|error| error.to_string())?
        {
            view.validate(ViewLimits::default())
                .map_err(|error| error.to_string())?;
        }
    }
    runtime
        .unload(&manifest.id)
        .map_err(|error| error.to_string())
}

pub(crate) fn is_regular_file(path: &Path, label: &str, diagnostics: &mut Vec<String>) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => true,
        Ok(_) => {
            diagnostics.push(format!(
                "error[file.type]: referenced {label} is not a regular file: {}",
                path.display()
            ));
            false
        }
        Err(error) => {
            diagnostics.push(format!(
                "error[file.missing]: referenced {label} {} is unavailable: {error}",
                path.display()
            ));
            false
        }
    }
}

fn write_registration(
    state_dir: &Path,
    registration: &DevelopmentRegistration,
) -> Result<(), String> {
    let directory = state_dir.join("dev");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = registration_path(state_dir, &registration.id);
    let temporary = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    let source = toml::to_string_pretty(registration).map_err(|error| error.to_string())?;
    fs::write(&temporary, source).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &path).map_err(|error| error.to_string())
}

fn read_registration(
    state_dir: &Path,
    id: &ExtensionId,
) -> Result<DevelopmentRegistration, String> {
    let path = registration_path(state_dir, id);
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    toml::from_str(&source).map_err(|error| format!("invalid registration: {error}"))
}

fn registration_path(state_dir: &Path, id: &ExtensionId) -> PathBuf {
    state_dir.join("dev").join(format!("{id}.toml"))
}

fn log_path(state_dir: &Path, id: &ExtensionId) -> PathBuf {
    state_dir.join("logs").join(format!("{id}.log"))
}

fn append_log(state_dir: &Path, id: &ExtensionId, message: &str) -> Result<(), String> {
    let path = log_path(state_dir, id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    writeln!(log, "{} {message}", unix_timestamp()).map_err(|error| error.to_string())
}

pub fn follow_log(id: &ExtensionId, state_dir: &Path) {
    let path = log_path(state_dir, id);
    let mut offset = fs::metadata(&path).map_or(0, |metadata| metadata.len());
    loop {
        thread::sleep(Duration::from_millis(250));
        let Ok(mut file) = File::open(&path) else {
            continue;
        };
        let Ok(metadata) = file.metadata() else {
            continue;
        };
        if metadata.len() < offset {
            offset = 0;
        }
        if metadata.len() == offset {
            continue;
        }
        use std::io::Seek;
        let _ = file.seek(std::io::SeekFrom::Start(offset));
        let mut new_content = String::new();
        if file.read_to_string(&mut new_content).is_ok() {
            print!("{new_content}");
            offset = metadata.len();
        }
    }
}

pub fn default_extension_state_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path).join("shilpo/extensions");
    }
    std::env::var_os("HOME").map_or_else(
        || PathBuf::from(".local/state/shilpo/extensions"),
        |home| PathBuf::from(home).join(".local/state/shilpo/extensions"),
    )
}

pub fn development_registrations(state_dir: &Path) -> (Vec<DevelopmentRegistration>, Vec<String>) {
    let directory = state_dir.join("dev");
    let Ok(entries) = fs::read_dir(&directory) else {
        return (Vec::new(), Vec::new());
    };
    let mut registrations = Vec::new();
    let mut diagnostics = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        match fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|source| toml::from_str(&source).map_err(|error| error.to_string()))
        {
            Ok(registration) => registrations.push(registration),
            Err(error) => diagnostics.push(format!(
                "ignored invalid development registration {}: {error}",
                path.display()
            )),
        }
    }
    registrations.sort_by(|left: &DevelopmentRegistration, right| left.id.cmp(&right.id));
    (registrations, diagnostics)
}

fn parse_id(value: Option<&String>) -> Option<ExtensionId> {
    let Some(value) = value else {
        eprintln!("extension ID is required");
        return None;
    };
    match ExtensionId::new(value) {
        Ok(id) => Some(id),
        Err(error) => {
            eprintln!("{error}");
            None
        }
    }
}

fn print_result(result: &ExtensionCliResult) {
    for diagnostic in &result.diagnostics {
        if diagnostic.starts_with("error[") {
            eprintln!("{diagnostic}");
        } else {
            println!("{diagnostic}");
        }
    }
}

fn print_usage() {
    eprintln!(
        "Usage: shilpo ext <build|check|pack|sign|keygen|dev|reload|logs|list|search|info|install|update|enable|disable|approve|rollback|uninstall|channel|check-updates|source|refresh-sources> [arguments]"
    );
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
