use crate::cli::{ExtensionCliResult, is_regular_file};
use serde::{Deserialize, Serialize};
use shilpo_ext_api::ExtensionManifest;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionLanguage {
    Typescript,
    Rust,
}

impl std::fmt::Display for ExtensionLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Typescript => write!(f, "typescript"),
            Self::Rust => write!(f, "rust"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionProjectConfig {
    pub language: ExtensionLanguage,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(rename = "crate", default)]
    pub crate_dir: Option<String>,
}

type ShilpoExtJson = ExtensionProjectConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBuildConfig {
    pub language: ExtensionLanguage,
    pub entry: Option<PathBuf>,
    pub crate_dir: Option<PathBuf>,
    pub manifest: ExtensionManifest,
    pub library_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

impl ProcessCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait ProcessRunner: Send + Sync {
    fn run(&self, command: &ProcessCommand) -> Result<ProcessOutput, String>;
    fn which(&self, binary_name: &str) -> Option<PathBuf>;
}

pub struct OsProcessRunner;

impl ProcessRunner for OsProcessRunner {
    fn run(&self, command: &ProcessCommand) -> Result<ProcessOutput, String> {
        let mut cmd = std::process::Command::new(&command.program);
        cmd.args(&command.args);
        if let Some(cwd) = &command.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &command.env {
            cmd.env(k, v);
        }
        let output = cmd
            .output()
            .map_err(|e| format!("failed to spawn '{}': {e}", command.program))?;
        Ok(ProcessOutput {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    fn which(&self, binary_name: &str) -> Option<PathBuf> {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                let candidate = dir.join(binary_name);
                if candidate.is_file() {
                    return Some(candidate);
                }
                #[cfg(windows)]
                {
                    let candidate_exe = dir.join(format!("{binary_name}.exe"));
                    if candidate_exe.is_file() {
                        return Some(candidate_exe);
                    }
                    let candidate_cmd = dir.join(format!("{binary_name}.cmd"));
                    if candidate_cmd.is_file() {
                        return Some(candidate_cmd);
                    }
                }
            }
        }
        None
    }
}

pub fn validate_safe_relative_path(path_str: &str, label: &str) -> Result<PathBuf, String> {
    if path_str.trim().is_empty() {
        return Err(format!("error[path.empty]: {label} path cannot be empty"));
    }
    let path = Path::new(path_str);
    if path.is_absolute() {
        return Err(format!(
            "error[path.absolute]: {label} path '{path_str}' must be project-relative"
        ));
    }
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(format!(
                    "error[path.traversal]: {label} path '{path_str}' contains parent traversal ('..')"
                ));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(format!(
                    "error[path.absolute]: {label} path '{path_str}' must be project-relative"
                ));
            }
            _ => {}
        }
    }
    Ok(path.to_path_buf())
}

pub fn ensure_within_root(root: &Path, target: &Path, label: &str) -> Result<(), String> {
    let canonical_root = root.canonicalize().map_err(|e| {
        format!(
            "error[path.root]: failed to resolve project root {}: {e}",
            root.display()
        )
    })?;
    let canonical_target = target.canonicalize().map_err(|e| {
        format!(
            "error[path.target]: failed to resolve {label} {}: {e}",
            target.display()
        )
    })?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(format!(
            "error[path.escape]: {label} {} escapes project root {}",
            target.display(),
            root.display()
        ));
    }
    Ok(())
}

pub fn resolve_project_config(dir: &Path) -> Result<ResolvedBuildConfig, Vec<String>> {
    let mut diagnostics = Vec::new();

    // 1. Manifest must exist and be valid
    let manifest_path = dir.join("extension.toml");
    if !is_regular_file(&manifest_path, "manifest", &mut diagnostics) {
        return Err(diagnostics);
    }
    if let Err(err) = ensure_within_root(dir, &manifest_path, "manifest") {
        diagnostics.push(err);
        return Err(diagnostics);
    }
    let manifest_source = match fs::read_to_string(&manifest_path) {
        Ok(source) => source,
        Err(error) => {
            diagnostics.push(format!(
                "error[manifest.read]: failed to read {}: {error}",
                manifest_path.display()
            ));
            return Err(diagnostics);
        }
    };
    let manifest = match ExtensionManifest::from_toml(&manifest_source) {
        Ok(m) => m,
        Err(error) => {
            diagnostics.push(format!("error[manifest.invalid]: {error}"));
            return Err(diagnostics);
        }
    };

    let library = match &manifest.library {
        Some(lib) if !lib.path.trim().is_empty() => lib,
        _ => {
            diagnostics.push("error[manifest.library]: manifest missing [library].path".into());
            return Err(diagnostics);
        }
    };

    let library_path = match validate_safe_relative_path(&library.path, "library") {
        Ok(p) => p,
        Err(e) => {
            diagnostics.push(e);
            return Err(diagnostics);
        }
    };

    // 2. Discover shilpo-ext.json or infer language
    let config_path = dir.join("shilpo-ext.json");
    let (language, entry, crate_dir) = if config_path.exists() {
        if !is_regular_file(&config_path, "project configuration", &mut diagnostics) {
            return Err(diagnostics);
        }
        if let Err(err) = ensure_within_root(dir, &config_path, "project configuration") {
            diagnostics.push(err);
            return Err(diagnostics);
        }
        let config_source = match fs::read_to_string(&config_path) {
            Ok(s) => s,
            Err(e) => {
                diagnostics.push(format!(
                    "error[config.read]: failed to read {}: {e}",
                    config_path.display()
                ));
                return Err(diagnostics);
            }
        };
        let cfg: ShilpoExtJson = match serde_json::from_str(&config_source) {
            Ok(c) => c,
            Err(e) => {
                diagnostics.push(format!(
                    "error[config.invalid]: {} is invalid: {e}",
                    config_path.display()
                ));
                return Err(diagnostics);
            }
        };

        match cfg.language {
            ExtensionLanguage::Typescript => {
                if cfg.crate_dir.is_some() {
                    diagnostics.push(
                        "error[config.invalid]: 'crate' property cannot be set for TypeScript language".into(),
                    );
                    return Err(diagnostics);
                }
                let entry_str = cfg.entry.unwrap_or_else(|| "src/extension.ts".into());
                let entry_path = match validate_safe_relative_path(&entry_str, "entry") {
                    Ok(p) => p,
                    Err(e) => {
                        diagnostics.push(e);
                        return Err(diagnostics);
                    }
                };
                let ext_str = entry_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if ext_str != "ts" && ext_str != "tsx" {
                    diagnostics.push(format!(
                        "error[config.entry]: entry file '{}' must end with .ts or .tsx",
                        entry_path.display()
                    ));
                    return Err(diagnostics);
                }
                (ExtensionLanguage::Typescript, Some(entry_path), None)
            }
            ExtensionLanguage::Rust => {
                if cfg.entry.is_some() {
                    diagnostics.push(
                        "error[config.invalid]: 'entry' property cannot be set for Rust language"
                            .into(),
                    );
                    return Err(diagnostics);
                }
                let crate_str = cfg.crate_dir.unwrap_or_else(|| ".".into());
                let crate_path = match validate_safe_relative_path(&crate_str, "crate") {
                    Ok(p) => p,
                    Err(e) => {
                        diagnostics.push(e);
                        return Err(diagnostics);
                    }
                };
                (ExtensionLanguage::Rust, None, Some(crate_path))
            }
        }
    } else {
        // Infer from project structure
        let ts_entry = dir.join("src/extension.ts");
        let rust_cargo = dir.join("Cargo.toml");
        let has_ts = ts_entry.is_file();
        let has_rust = rust_cargo.is_file();

        match (has_ts, has_rust) {
            (true, false) => (
                ExtensionLanguage::Typescript,
                Some(PathBuf::from("src/extension.ts")),
                None,
            ),
            (false, true) => (ExtensionLanguage::Rust, None, Some(PathBuf::from("."))),
            (true, true) => {
                diagnostics.push(
                    "error[build.config]: ambiguous project: found both src/extension.ts and Cargo.toml; specify \"language\" in shilpo-ext.json".into(),
                );
                return Err(diagnostics);
            }
            (false, false) => {
                diagnostics.push(
                    "error[build.config]: unable to infer project language: neither src/extension.ts nor Cargo.toml was found; specify \"language\" in shilpo-ext.json".into(),
                );
                return Err(diagnostics);
            }
        }
    };

    // 3. Validate source target existence and safety
    if let Some(entry_path) = &entry {
        let full_entry = dir.join(entry_path);
        if !is_regular_file(&full_entry, "TypeScript entry", &mut diagnostics) {
            return Err(diagnostics);
        }
        if let Err(err) = ensure_within_root(dir, &full_entry, "TypeScript entry") {
            diagnostics.push(err);
            return Err(diagnostics);
        }
    }

    if let Some(crate_path) = &crate_dir {
        let full_crate = dir.join(crate_path);
        if !full_crate.is_dir() {
            diagnostics.push(format!(
                "error[crate.missing]: crate directory {} does not exist",
                full_crate.display()
            ));
            return Err(diagnostics);
        }
        if let Err(err) = ensure_within_root(dir, &full_crate, "crate directory") {
            diagnostics.push(err);
            return Err(diagnostics);
        }
        let cargo_toml = full_crate.join("Cargo.toml");
        if !is_regular_file(&cargo_toml, "Cargo.toml", &mut diagnostics) {
            return Err(diagnostics);
        }
        if let Err(err) = ensure_within_root(dir, &cargo_toml, "Cargo.toml") {
            diagnostics.push(err);
            return Err(diagnostics);
        }
    }

    Ok(ResolvedBuildConfig {
        language,
        entry,
        crate_dir,
        manifest,
        library_path,
    })
}

pub fn find_canonical_wit_dir(project_dir: &Path) -> Option<PathBuf> {
    if let Some(env_dir) = std::env::var_os("SHILPO_WIT_DIR") {
        let path = PathBuf::from(env_dir);
        if path.join("extension.wit").is_file() {
            return Some(path);
        }
    }

    // Check ancestors of project_dir
    let mut current = Some(project_dir);
    while let Some(dir) = current {
        let candidate = dir.join("core/ext-api/wit");
        if candidate.join("extension.wit").is_file() {
            return Some(candidate);
        }
        current = dir.parent();
    }

    // Check ancestors of current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let mut current = Some(cwd.as_path());
        while let Some(dir) = current {
            let candidate = dir.join("core/ext-api/wit");
            if candidate.join("extension.wit").is_file() {
                return Some(candidate);
            }
            current = dir.parent();
        }
    }

    // Check ancestors of current executable
    if let Ok(exe) = std::env::current_exe() {
        let mut current = exe.parent();
        while let Some(dir) = current {
            let candidate = dir.join("core/ext-api/wit");
            if candidate.join("extension.wit").is_file() {
                return Some(candidate);
            }
            current = dir.parent();
        }
    }

    // Check standard installed XDG data directory
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        let candidate = PathBuf::from(data_home).join("shilpo/wit");
        if candidate.join("extension.wit").is_file() {
            return Some(candidate);
        }
    }
    let system_candidates = ["/usr/local/share/shilpo/wit", "/usr/share/shilpo/wit"];
    for &cand in &system_candidates {
        let path = PathBuf::from(cand);
        if path.join("extension.wit").is_file() {
            return Some(path);
        }
    }

    None
}

pub fn find_local_jco(project_dir: &Path) -> Option<PathBuf> {
    if let Some(env_jco) = std::env::var_os("SHILPO_JCO_BIN") {
        let path = PathBuf::from(env_jco);
        if is_safe_tool_path(&path, project_dir) && find_javascript_lockfile(project_dir).is_some()
        {
            return Some(path);
        }
    }

    let mut current = Some(project_dir);
    while let Some(dir) = current {
        let candidate = dir.join("node_modules/.bin/jco");
        if is_safe_tool_path(&candidate, dir) && find_javascript_lockfile(dir).is_some() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate_cmd = dir.join("node_modules/.bin/jco.cmd");
            if is_safe_tool_path(&candidate_cmd, dir) && find_javascript_lockfile(dir).is_some() {
                return Some(candidate_cmd);
            }
        }
        let candidate_js = dir.join("node_modules/@bytecodealliance/jco/bin/jco.js");
        if is_safe_tool_path(&candidate_js, dir) && find_javascript_lockfile(dir).is_some() {
            return Some(candidate_js);
        }
        current = dir.parent();
    }

    None
}

fn is_safe_tool_path(path: &Path, root: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(target) = path.canonicalize() else {
        return false;
    };
    target.starts_with(root) && target.is_file()
}

fn find_javascript_lockfile(project_dir: &Path) -> Option<PathBuf> {
    let mut current = Some(project_dir);
    while let Some(dir) = current {
        for name in [
            "package-lock.json",
            "npm-shrinkwrap.json",
            "pnpm-lock.yaml",
            "yarn.lock",
        ] {
            let candidate = dir.join(name);
            if candidate.is_file() && !candidate.is_symlink() {
                return Some(candidate);
            }
        }
        current = dir.parent();
    }
    None
}

fn determine_rust_artifact(
    crate_dir: &Path,
    release: bool,
    runner: &dyn ProcessRunner,
) -> Result<PathBuf, String> {
    let cargo_toml = crate_dir.join("Cargo.toml");
    let profile = if release { "release" } else { "debug" };

    // Try `cargo metadata --format-version 1 --no-deps --manifest-path <Cargo.toml>`
    let metadata_cmd = ProcessCommand::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
            cargo_toml.to_str().unwrap_or("Cargo.toml"),
        ])
        .cwd(crate_dir);

    if let Ok(output) = runner.run(&metadata_cmd)
        && output.success
        && let Ok(metadata) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        && let Some(target_dir_str) = metadata.get("target_directory").and_then(|v| v.as_str())
    {
        let target_dir = PathBuf::from(target_dir_str);
        let requested_manifest = cargo_toml.canonicalize().ok();
        if let Some(packages) = metadata.get("packages").and_then(|v| v.as_array()) {
            for pkg in packages {
                let package_manifest = pkg
                    .get("manifest_path")
                    .and_then(|v| v.as_str())
                    .and_then(|path| PathBuf::from(path).canonicalize().ok());
                if package_manifest != requested_manifest {
                    continue;
                }
                if let Some(targets) = pkg.get("targets").and_then(|v| v.as_array()) {
                    for tgt in targets {
                        let kinds: HashSet<&str> = tgt
                            .get("kind")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|s| s.as_str()).collect())
                            .unwrap_or_default();
                        if kinds.contains("cdylib")
                            && let Some(target_name) = tgt.get("name").and_then(|v| v.as_str())
                        {
                            let artifact_name = format!("{target_name}.wasm");
                            return Ok(target_dir
                                .join("wasm32-wasip2")
                                .join(profile)
                                .join(artifact_name));
                        }
                    }
                }
            }
        }
    }

    // Fallback parsing of Cargo.toml
    let toml_source = fs::read_to_string(&cargo_toml)
        .map_err(|e| format!("failed to read Cargo.toml at {}: {e}", cargo_toml.display()))?;
    let toml_value: toml::Value =
        toml::from_str(&toml_source).map_err(|e| format!("failed to parse Cargo.toml: {e}"))?;

    let lib_name = toml_value
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(|name| name.as_str())
        .or_else(|| {
            toml_value
                .get("package")
                .and_then(|pkg| pkg.get("name"))
                .and_then(|name| name.as_str())
        })
        .ok_or_else(|| "could not determine crate library name from Cargo.toml".to_string())?;

    let sanitized_lib_name = lib_name.replace('-', "_");
    let target_dir = cargo_target_dir(crate_dir);
    Ok(target_dir
        .join("wasm32-wasip2")
        .join(profile)
        .join(format!("{sanitized_lib_name}.wasm")))
}

fn cargo_target_dir(crate_dir: &Path) -> PathBuf {
    if let Some(target_dir) = std::env::var_os("CARGO_TARGET_DIR") {
        let target_dir = PathBuf::from(target_dir);
        return if target_dir.is_absolute() {
            target_dir
        } else {
            crate_dir.join(target_dir)
        };
    }

    for config_name in [".cargo/config.toml", ".cargo/config"] {
        let config_path = crate_dir.join(config_name);
        if let Ok(source) = fs::read_to_string(&config_path)
            && let Ok(config) = toml::from_str::<toml::Value>(&source)
            && let Some(target_dir) = config
                .get("build")
                .and_then(|build| build.get("target-dir"))
            && let Some(target_dir) = target_dir.as_str()
        {
            let target_dir = PathBuf::from(target_dir);
            return if target_dir.is_absolute() {
                target_dir
            } else {
                crate_dir.join(target_dir)
            };
        }
    }
    crate_dir.join("target")
}

fn temporary_destination(dest_dir: &Path, final_dest: &Path) -> PathBuf {
    let name = final_dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("component.wasm");
    loop {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = dest_dir.join(format!(".{name}.{}.{}.tmp", std::process::id(), counter));
        if !candidate.exists() {
            return candidate;
        }
    }
}

fn build_typescript(
    dir: &Path,
    config: &ResolvedBuildConfig,
    runner: &dyn ProcessRunner,
    diagnostics: &mut Vec<String>,
) -> Result<PathBuf, ()> {
    let entry_path = config
        .entry
        .as_ref()
        .expect("TypeScript configuration must have entry");
    let full_entry = dir.join(entry_path);

    if runner.which("node").is_none() {
        diagnostics.push(
            "error[build.toolchain]: Node.js is required to build TypeScript extensions but was not found in PATH"
                .into(),
        );
        return Err(());
    }

    let jco_path = match find_local_jco(dir) {
        Some(p) => p,
        None => {
            diagnostics.push(
                "error[build.toolchain]: project-local, lockfile-backed JCO executable not found; install it locally with 'npm install --save-dev @bytecodealliance/jco' and commit the lockfile".into(),
            );
            return Err(());
        }
    };

    let wit_dir = match find_canonical_wit_dir(dir) {
        Some(p) => p,
        None => {
            diagnostics.push(
                "error[build.wit]: canonical WIT directory containing extension.wit was not found; ensure core/ext-api/wit exists or set SHILPO_WIT_DIR".into(),
            );
            return Err(());
        }
    };

    let final_dest = dir.join(&config.library_path);
    let dest_dir = final_dest.parent().unwrap_or(dir);
    if let Err(e) = fs::create_dir_all(dest_dir) {
        diagnostics.push(format!(
            "error[build.fs]: failed to create destination directory {}: {e}",
            dest_dir.display()
        ));
        return Err(());
    }

    let temp_dest = temporary_destination(dest_dir, &final_dest);

    let jco_abs = fs::canonicalize(&jco_path).unwrap_or_else(|_| jco_path.clone());
    let entry_abs = fs::canonicalize(&full_entry).unwrap_or_else(|_| full_entry.clone());
    let wit_abs = fs::canonicalize(&wit_dir).unwrap_or_else(|_| wit_dir.clone());
    let temp_dest_abs = fs::canonicalize(dest_dir)
        .map(|dir| {
            dir.join(
                temp_dest
                    .file_name()
                    .expect("temporary destination has a name"),
            )
        })
        .unwrap_or_else(|_| temp_dest.clone());

    let cmd = ProcessCommand::new("node")
        .arg(jco_abs.to_str().unwrap_or("jco"))
        .arg("componentize")
        .arg(entry_abs.to_str().unwrap_or("entry.ts"))
        .arg("--wit")
        .arg(wit_abs.to_str().unwrap_or("wit"))
        .arg("--world-name")
        .arg("extension")
        .arg("-o")
        .arg(temp_dest_abs.to_str().unwrap_or("temp.wasm"))
        .cwd(dir);

    let output = match runner.run(&cmd) {
        Ok(out) => out,
        Err(e) => {
            let _ = fs::remove_file(&temp_dest_abs);
            diagnostics.push(format!("error[build.spawn]: {e}"));
            return Err(());
        }
    };

    if !output.stderr.is_empty() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        for line in stderr_str.lines() {
            if !line.trim().is_empty() {
                diagnostics.push(line.to_string());
            }
        }
    }

    if !output.stdout.is_empty() {
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        for line in stdout_str.lines() {
            if !line.trim().is_empty() {
                diagnostics.push(line.to_string());
            }
        }
    }

    if !output.success {
        let _ = fs::remove_file(&temp_dest_abs);
        diagnostics.push(format!(
            "error[build.failed]: JCO componentize exited with status {}",
            output.exit_code.unwrap_or(1)
        ));
        return Err(());
    }

    if !temp_dest_abs.is_file() {
        diagnostics.push(format!(
            "error[build.artifact]: expected build output at {} was not created",
            temp_dest_abs.display()
        ));
        let _ = fs::remove_file(&temp_dest_abs);
        return Err(());
    }

    if let Err(e) = fs::rename(&temp_dest_abs, &final_dest) {
        let _ = fs::remove_file(&temp_dest_abs);
        diagnostics.push(format!(
            "error[build.activate]: failed to activate built component at {}: {e}",
            final_dest.display()
        ));
        return Err(());
    }

    Ok(final_dest)
}

fn build_rust(
    dir: &Path,
    config: &ResolvedBuildConfig,
    release: bool,
    runner: &dyn ProcessRunner,
    diagnostics: &mut Vec<String>,
) -> Result<PathBuf, ()> {
    let crate_rel = config
        .crate_dir
        .as_ref()
        .expect("Rust configuration must have crate_dir");
    let full_crate = dir.join(crate_rel);

    if runner.which("cargo").is_none() {
        diagnostics.push(
            "error[build.toolchain]: Cargo is required to build Rust extensions but was not found in PATH"
                .into(),
        );
        return Err(());
    }

    // Check cargo-component availability
    let cargo_comp_available = runner.which("cargo-component").is_some() || {
        let check_cmd = ProcessCommand::new("cargo")
            .args(["component", "--version"])
            .cwd(&full_crate);
        runner.run(&check_cmd).is_ok_and(|out| out.success)
    };

    if !cargo_comp_available {
        diagnostics.push(
            "error[build.toolchain]: cargo-component is required to build Rust extensions; install it with 'cargo install cargo-component'".into(),
        );
        return Err(());
    }

    let mut cmd = ProcessCommand::new("cargo");
    cmd = cmd.arg("component").arg("build");
    if release {
        cmd = cmd.arg("--release");
    }
    cmd = cmd.cwd(&full_crate);

    let output = match runner.run(&cmd) {
        Ok(out) => out,
        Err(e) => {
            diagnostics.push(format!("error[build.spawn]: {e}"));
            return Err(());
        }
    };

    if !output.stderr.is_empty() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        for line in stderr_str.lines() {
            if !line.trim().is_empty() {
                diagnostics.push(line.to_string());
            }
        }
    }

    if !output.stdout.is_empty() {
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        for line in stdout_str.lines() {
            if !line.trim().is_empty() {
                diagnostics.push(line.to_string());
            }
        }
    }

    if !output.success {
        diagnostics.push(format!(
            "error[build.failed]: cargo component build exited with status {}",
            output.exit_code.unwrap_or(1)
        ));
        return Err(());
    }

    let produced_artifact = match determine_rust_artifact(&full_crate, release, runner) {
        Ok(p) => p,
        Err(e) => {
            diagnostics.push(format!("error[build.artifact]: {e}"));
            return Err(());
        }
    };

    if !produced_artifact.is_file() {
        diagnostics.push(format!(
            "error[build.artifact]: expected build artifact at {} was not found",
            produced_artifact.display()
        ));
        return Err(());
    }

    let final_dest = dir.join(&config.library_path);
    let dest_dir = final_dest.parent().unwrap_or(dir);
    if let Err(e) = fs::create_dir_all(dest_dir) {
        diagnostics.push(format!(
            "error[build.fs]: failed to create destination directory {}: {e}",
            dest_dir.display()
        ));
        return Err(());
    }

    let temp_dest = temporary_destination(dest_dir, &final_dest);

    if let Err(e) = fs::copy(&produced_artifact, &temp_dest) {
        let _ = fs::remove_file(&temp_dest);
        diagnostics.push(format!(
            "error[build.copy]: failed to stage artifact from {} to {}: {e}",
            produced_artifact.display(),
            temp_dest.display()
        ));
        return Err(());
    }

    if let Err(e) = fs::rename(&temp_dest, &final_dest) {
        let _ = fs::remove_file(&temp_dest);
        diagnostics.push(format!(
            "error[build.activate]: failed to activate built component at {}: {e}",
            final_dest.display()
        ));
        return Err(());
    }

    Ok(final_dest)
}

pub fn build_extension(
    dir: &Path,
    release: bool,
    runner: &dyn ProcessRunner,
) -> ExtensionCliResult {
    let config = match resolve_project_config(dir) {
        Ok(c) => c,
        Err(diagnostics) => {
            return ExtensionCliResult {
                success: false,
                extension_id: None,
                artifact: None,
                diagnostics,
            };
        }
    };

    let extension_id = config.manifest.id.to_string();
    let mut diagnostics = vec![format!(
        "info[manifest.valid]: '{}' {}",
        config.manifest.id, config.manifest.version
    )];

    let build_res = match config.language {
        ExtensionLanguage::Typescript => build_typescript(dir, &config, runner, &mut diagnostics),
        ExtensionLanguage::Rust => build_rust(dir, &config, release, runner, &mut diagnostics),
    };

    match build_res {
        Ok(artifact) => {
            diagnostics.push(format!(
                "info[build.success]: built {} extension component at {}",
                config.language,
                config.library_path.display()
            ));
            ExtensionCliResult {
                success: true,
                extension_id: Some(extension_id),
                artifact: Some(artifact),
                diagnostics,
            }
        }
        Err(()) => ExtensionCliResult {
            success: false,
            extension_id: Some(extension_id),
            artifact: None,
            diagnostics,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tempfile::tempdir;

    struct MockProcessRunner {
        commands: Mutex<Vec<ProcessCommand>>,
        responses: Mutex<HashMap<String, Result<ProcessOutput, String>>>,
        available_binaries: Mutex<HashSet<String>>,
    }

    impl MockProcessRunner {
        fn new() -> Self {
            let mut available = HashSet::new();
            available.insert("node".into());
            available.insert("cargo".into());
            available.insert("cargo-component".into());
            Self {
                commands: Mutex::new(Vec::new()),
                responses: Mutex::new(HashMap::new()),
                available_binaries: Mutex::new(available),
            }
        }

        fn with_binary(self, binary: &str, available: bool) -> Self {
            let mut bins = self.available_binaries.lock().unwrap();
            if available {
                bins.insert(binary.into());
            } else {
                bins.remove(binary);
            }
            drop(bins);
            self
        }

        fn with_response(self, program: &str, output: Result<ProcessOutput, String>) -> Self {
            self.responses
                .lock()
                .unwrap()
                .insert(program.into(), output);
            self
        }

        fn recorded_commands(&self) -> Vec<ProcessCommand> {
            self.commands.lock().unwrap().clone()
        }
    }

    impl ProcessRunner for MockProcessRunner {
        fn run(&self, command: &ProcessCommand) -> Result<ProcessOutput, String> {
            self.commands.lock().unwrap().push(command.clone());
            if let Some(resp) = self.responses.lock().unwrap().get(&command.program) {
                return resp.clone();
            }
            if command.program == "cargo"
                && command.args.first().is_some_and(|a| a == "component")
                && !self
                    .available_binaries
                    .lock()
                    .unwrap()
                    .contains("cargo-component")
            {
                return Ok(ProcessOutput {
                    success: false,
                    exit_code: Some(101),
                    stdout: Vec::new(),
                    stderr: b"error: no such command: `component`".to_vec(),
                });
            }
            Ok(ProcessOutput {
                success: true,
                exit_code: Some(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }

        fn which(&self, binary_name: &str) -> Option<PathBuf> {
            if self
                .available_binaries
                .lock()
                .unwrap()
                .contains(binary_name)
            {
                Some(PathBuf::from(format!("/usr/bin/{binary_name}")))
            } else {
                None
            }
        }
    }

    fn write_manifest(dir: &Path, id: &str, library_path: &str) {
        let manifest = format!(
            r#"
            schema_version = 1
            id = "{id}"
            name = "Test Extension"
            version = "0.1.0"
            api_version = "0.1.0"
            min_shilpo_version = "0.1.0"
            authors = ["Shilpo Contributors"]
            license = "MIT"

            [library]
            path = "{library_path}"
            "#
        );
        fs::write(dir.join("extension.toml"), manifest).unwrap();
    }

    #[test]
    fn test_resolve_project_config_shilpo_ext_json_typescript_default() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::write(dir.join("shilpo-ext.json"), r#"{"language": "typescript"}"#).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/extension.ts"), "export {}").unwrap();

        let config = resolve_project_config(dir).unwrap();
        assert_eq!(config.language, ExtensionLanguage::Typescript);
        assert_eq!(config.entry, Some(PathBuf::from("src/extension.ts")));
        assert_eq!(config.crate_dir, None);
        assert_eq!(config.library_path, PathBuf::from("extension.wasm"));
    }

    #[test]
    fn test_resolve_project_config_shilpo_ext_json_typescript_custom_entry() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::write(
            dir.join("shilpo-ext.json"),
            r#"{"language": "typescript", "entry": "src/custom.tsx"}"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/custom.tsx"), "export {}").unwrap();

        let config = resolve_project_config(dir).unwrap();
        assert_eq!(config.language, ExtensionLanguage::Typescript);
        assert_eq!(config.entry, Some(PathBuf::from("src/custom.tsx")));
        assert_eq!(config.crate_dir, None);
    }

    #[test]
    fn test_resolve_project_config_shilpo_ext_json_rust_default() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::write(dir.join("shilpo-ext.json"), r#"{"language": "rust"}"#).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"test-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"",
        )
        .unwrap();

        let config = resolve_project_config(dir).unwrap();
        assert_eq!(config.language, ExtensionLanguage::Rust);
        assert_eq!(config.crate_dir, Some(PathBuf::from(".")));
        assert_eq!(config.entry, None);
    }

    #[test]
    fn test_resolve_project_config_shilpo_ext_json_rust_custom_crate() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::write(
            dir.join("shilpo-ext.json"),
            r#"{"language": "rust", "crate": "crates/guest"}"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("crates/guest")).unwrap();
        fs::write(
            dir.join("crates/guest/Cargo.toml"),
            "[package]\nname = \"guest-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"",
        )
        .unwrap();

        let config = resolve_project_config(dir).unwrap();
        assert_eq!(config.language, ExtensionLanguage::Rust);
        assert_eq!(config.crate_dir, Some(PathBuf::from("crates/guest")));
        assert_eq!(config.entry, None);
    }

    #[test]
    fn test_resolve_project_config_inferred_typescript() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/extension.ts"), "export {}").unwrap();

        let config = resolve_project_config(dir).unwrap();
        assert_eq!(config.language, ExtensionLanguage::Typescript);
        assert_eq!(config.entry, Some(PathBuf::from("src/extension.ts")));
        assert_eq!(config.crate_dir, None);
    }

    #[test]
    fn test_resolve_project_config_inferred_rust() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"test-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"",
        )
        .unwrap();

        let config = resolve_project_config(dir).unwrap();
        assert_eq!(config.language, ExtensionLanguage::Rust);
        assert_eq!(config.crate_dir, Some(PathBuf::from(".")));
        assert_eq!(config.entry, None);
    }

    #[test]
    fn test_resolve_project_config_ambiguous_fails() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/extension.ts"), "export {}").unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"test-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"",
        )
        .unwrap();

        let err = resolve_project_config(dir).unwrap_err();
        assert!(
            err.iter()
                .any(|d| d.contains("ambiguous project: found both"))
        );
    }

    #[test]
    fn test_resolve_project_config_neither_fails() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");

        let err = resolve_project_config(dir).unwrap_err();
        assert!(
            err.iter()
                .any(|d| d.contains("unable to infer project language"))
        );
    }

    #[test]
    fn test_resolve_project_config_rejects_parent_traversal() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::write(
            dir.join("shilpo-ext.json"),
            r#"{"language": "typescript", "entry": "../escaped.ts"}"#,
        )
        .unwrap();

        let err = resolve_project_config(dir).unwrap_err();
        assert!(err.iter().any(|d| d.contains("parent traversal")));
    }

    #[test]
    fn test_resolve_project_config_rejects_absolute_path() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::write(
            dir.join("shilpo-ext.json"),
            r#"{"language": "typescript", "entry": "/root/secret.ts"}"#,
        )
        .unwrap();

        let err = resolve_project_config(dir).unwrap_err();
        assert!(err.iter().any(|d| d.contains("must be project-relative")));
    }

    #[test]
    fn test_resolve_project_config_rejects_non_ts_extension() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::write(
            dir.join("shilpo-ext.json"),
            r#"{"language": "typescript", "entry": "src/main.py"}"#,
        )
        .unwrap();

        let err = resolve_project_config(dir).unwrap_err();
        assert!(err.iter().any(|d| d.contains("must end with .ts or .tsx")));
    }

    #[test]
    fn test_resolve_project_config_rejects_mismatched_properties() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.test", "extension.wasm");
        fs::write(
            dir.join("shilpo-ext.json"),
            r#"{"language": "typescript", "crate": "."}"#,
        )
        .unwrap();

        let err = resolve_project_config(dir).unwrap_err();
        assert!(
            err.iter()
                .any(|d| d.contains("'crate' property cannot be set for TypeScript"))
        );
    }

    #[test]
    fn test_typescript_build_with_mock_runner_success() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.ts-test", "dist/output.wasm");
        fs::write(
            dir.join("shilpo-ext.json"),
            r#"{"language": "typescript", "entry": "src/extension.ts"}"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/extension.ts"), "export {}").unwrap();

        // Local JCO
        fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        fs::write(dir.join("node_modules/.bin/jco"), "#!/usr/bin/env node").unwrap();
        fs::write(dir.join("package-lock.json"), "{}").unwrap();

        // WIT dir
        fs::create_dir_all(dir.join("core/ext-api/wit")).unwrap();
        fs::write(
            dir.join("core/ext-api/wit/extension.wit"),
            "package test:ext;",
        )
        .unwrap();

        // When node is invoked, create the output file at the temporary path specified in args
        struct CustomNodeRunner;
        impl ProcessRunner for CustomNodeRunner {
            fn run(&self, command: &ProcessCommand) -> Result<ProcessOutput, String> {
                if command.program == "node"
                    && let Some(pos) = command.args.iter().position(|a| a == "-o")
                    && let Some(out_path) = command.args.get(pos + 1)
                {
                    fs::write(out_path, b"\x00asm\x0d\x00\x01\x00FAKE_WASM").unwrap();
                }
                Ok(ProcessOutput {
                    success: true,
                    exit_code: Some(0),
                    stdout: b"Componentized successfully\n".to_vec(),
                    stderr: Vec::new(),
                })
            }
            fn which(&self, binary_name: &str) -> Option<PathBuf> {
                if binary_name == "node" {
                    Some(PathBuf::from("/usr/bin/node"))
                } else {
                    None
                }
            }
        }

        let res = build_extension(dir, false, &CustomNodeRunner);
        assert!(res.success);
        assert_eq!(res.extension_id.as_deref(), Some("org.shilpo.ts-test"));
        assert_eq!(res.artifact, Some(dir.join("dist/output.wasm")));
        assert!(dir.join("dist/output.wasm").is_file());
        assert_eq!(
            fs::read(dir.join("dist/output.wasm")).unwrap(),
            b"\x00asm\x0d\x00\x01\x00FAKE_WASM"
        );
    }

    #[test]
    fn test_typescript_build_missing_node() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.ts-test", "extension.wasm");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/extension.ts"), "export {}").unwrap();
        fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        fs::write(dir.join("node_modules/.bin/jco"), "#!/usr/bin/env node").unwrap();
        fs::write(dir.join("package-lock.json"), "{}").unwrap();

        let mock_runner = MockProcessRunner::new().with_binary("node", false);
        let res = build_extension(dir, false, &mock_runner);
        assert!(!res.success);
        assert!(
            res.diagnostics
                .iter()
                .any(|d| d.contains("Node.js is required"))
        );
    }

    #[test]
    fn test_typescript_build_missing_jco() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.ts-test", "extension.wasm");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/extension.ts"), "export {}").unwrap();

        let mock_runner = MockProcessRunner::new().with_binary("node", true);
        let res = build_extension(dir, false, &mock_runner);
        assert!(!res.success);
        assert!(
            res.diagnostics
                .iter()
                .any(|d| d.contains("project-local"))
        );
    }

    #[test]
    fn test_typescript_build_failure_preserves_previous_component() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.ts-test", "extension.wasm");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/extension.ts"), "export {}").unwrap();
        fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        fs::write(dir.join("node_modules/.bin/jco"), "#!/usr/bin/env node").unwrap();
        fs::write(dir.join("package-lock.json"), "{}").unwrap();
        fs::create_dir_all(dir.join("core/ext-api/wit")).unwrap();
        fs::write(
            dir.join("core/ext-api/wit/extension.wit"),
            "package test:ext;",
        )
        .unwrap();

        // Write existing component
        let original_bytes = b"ORIGINAL_WORKING_COMPONENT";
        fs::write(dir.join("extension.wasm"), original_bytes).unwrap();

        // Mock runner simulates JCO compiler error
        let mock_runner = MockProcessRunner::new().with_response(
            "node",
            Ok(ProcessOutput {
                success: false,
                exit_code: Some(1),
                stdout: Vec::new(),
                stderr: b"SyntaxError: unexpected token".to_vec(),
            }),
        );

        let res = build_extension(dir, false, &mock_runner);
        assert!(!res.success);
        assert!(
            res.diagnostics
                .iter()
                .any(|d| d.contains("JCO componentize exited with status 1"))
        );

        // Previous component must be preserved
        assert_eq!(
            fs::read(dir.join("extension.wasm")).unwrap(),
            original_bytes
        );

        // No temp files left in directory
        let entries: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            entries.is_empty(),
            "Temporary files must be cleaned up on failure"
        );
    }

    #[test]
    fn test_rust_build_with_mock_runner_success() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.rust-test", "extension.wasm");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"my-extension\"\nversion = \"0.1.0\"\nedition = \"2024\"",
        )
        .unwrap();

        // Pre-create the expected build output
        let target_wasm = dir.join("target/wasm32-wasip2/debug/my_extension.wasm");
        fs::create_dir_all(target_wasm.parent().unwrap()).unwrap();
        fs::write(&target_wasm, b"\x00asm\x0d\x00\x01\x00RUST_COMPONENT").unwrap();

        let mock_runner = MockProcessRunner::new();
        let res = build_extension(dir, false, &mock_runner);
        assert!(res.success);
        assert_eq!(res.extension_id.as_deref(), Some("org.shilpo.rust-test"));
        assert_eq!(res.artifact, Some(dir.join("extension.wasm")));
        assert_eq!(
            fs::read(dir.join("extension.wasm")).unwrap(),
            b"\x00asm\x0d\x00\x01\x00RUST_COMPONENT"
        );

        let commands = mock_runner.recorded_commands();
        let build_cmd = commands
            .iter()
            .find(|c| c.program == "cargo" && c.args.contains(&"build".to_string()))
            .expect("cargo component build should be invoked");
        assert_eq!(build_cmd.args, vec!["component", "build"]);
        assert_eq!(build_cmd.cwd, Some(dir.to_path_buf()));
    }

    #[test]
    fn test_rust_build_with_release_flag() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.rust-test", "extension.wasm");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"my-extension\"\nversion = \"0.1.0\"\nedition = \"2024\"",
        )
        .unwrap();

        // Pre-create the expected release build output
        let target_wasm = dir.join("target/wasm32-wasip2/release/my_extension.wasm");
        fs::create_dir_all(target_wasm.parent().unwrap()).unwrap();
        fs::write(&target_wasm, b"\x00asm\x0d\x00\x01\x00RELEASE_COMPONENT").unwrap();

        let mock_runner = MockProcessRunner::new();
        let res = build_extension(dir, true, &mock_runner);
        assert!(res.success);
        assert_eq!(res.artifact, Some(dir.join("extension.wasm")));

        let commands = mock_runner.recorded_commands();
        let build_cmd = commands
            .iter()
            .find(|c| c.program == "cargo" && c.args.contains(&"build".to_string()))
            .expect("cargo component build --release should be invoked");
        assert_eq!(build_cmd.args, vec!["component", "build", "--release"]);
    }

    #[test]
    fn test_rust_build_missing_toolchains() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.rust-test", "extension.wasm");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"my-extension\"\nversion = \"0.1.0\"\nedition = \"2024\"",
        )
        .unwrap();

        let mock_no_cargo = MockProcessRunner::new().with_binary("cargo", false);
        let res = build_extension(dir, false, &mock_no_cargo);
        assert!(!res.success);
        assert!(
            res.diagnostics
                .iter()
                .any(|d| d.contains("Cargo is required"))
        );

        let mock_no_component = MockProcessRunner::new().with_binary("cargo-component", false);
        let res2 = build_extension(dir, false, &mock_no_component);
        assert!(!res2.success);
        assert!(
            res2.diagnostics
                .iter()
                .any(|d| d.contains("cargo-component is required"))
        );
    }

    #[test]
    fn test_rust_build_failure_preserves_previous_component() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.rust-test", "extension.wasm");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"my-extension\"\nversion = \"0.1.0\"\nedition = \"2024\"",
        )
        .unwrap();

        // Existing component
        let original_bytes = b"ORIGINAL_RUST_COMPONENT";
        fs::write(dir.join("extension.wasm"), original_bytes).unwrap();

        // Mock build failure
        let mock_runner = MockProcessRunner::new().with_response(
            "cargo",
            Ok(ProcessOutput {
                success: false,
                exit_code: Some(101),
                stdout: Vec::new(),
                stderr: b"error[E0425]: cannot find value `foo` in this scope".to_vec(),
            }),
        );

        let res = build_extension(dir, false, &mock_runner);
        assert!(!res.success);
        assert_eq!(
            fs::read(dir.join("extension.wasm")).unwrap(),
            original_bytes
        );

        // No temp files left
        let entries: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_typescript_build_cleans_temp_when_jco_does_not_write_output() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        write_manifest(dir, "org.shilpo.ts-no-output", "extension.wasm");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/extension.ts"), "export {}").unwrap();
        fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        fs::write(dir.join("node_modules/.bin/jco"), "#!/usr/bin/env node").unwrap();
        fs::write(dir.join("package-lock.json"), "{}").unwrap();
        fs::create_dir_all(dir.join("core/ext-api/wit")).unwrap();
        fs::write(
            dir.join("core/ext-api/wit/extension.wit"),
            "package test:ext;",
        )
        .unwrap();

        let result = build_extension(dir, false, &MockProcessRunner::new());
        assert!(!result.success);
        let temporary_files = fs::read_dir(dir)
            .unwrap()
            .flat_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(temporary_files, 0);
    }

    #[cfg(unix)]
    #[test]
    fn test_local_jco_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let dir = temp.path();
        fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        fs::write(dir.join("package-lock.json"), "{}").unwrap();
        let outside = tempdir().unwrap();
        let outside_jco = outside.path().join("jco");
        fs::write(&outside_jco, "#!/usr/bin/env node").unwrap();
        symlink(&outside_jco, dir.join("node_modules/.bin/jco")).unwrap();

        assert!(find_local_jco(dir).is_none());
    }

    #[test]
    fn test_rust_artifact_selection_matches_requested_manifest() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        let crate_dir = dir.join("crate");
        fs::create_dir_all(&crate_dir).unwrap();
        let cargo_toml = crate_dir.join("Cargo.toml");
        fs::write(
            &cargo_toml,
            "[package]\nname = \"requested\"\nversion = \"0.1.0\"\n[lib]\ncrate-type = [\"cdylib\"]",
        )
        .unwrap();
        let target = dir.join("target");
        let artifact = target.join("wasm32-wasip2/debug/requested.wasm");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(&artifact, b"component").unwrap();
        let manifest_path = cargo_toml.canonicalize().unwrap();
        let metadata = serde_json::json!({
            "target_directory": target,
            "packages": [
                {
                    "manifest_path": dir.join("other/Cargo.toml"),
                    "targets": [{"kind": ["cdylib"], "name": "other"}]
                },
                {
                    "manifest_path": manifest_path,
                    "targets": [{"kind": ["cdylib"], "name": "requested"}]
                }
            ]
        });

        struct MetadataRunner {
            metadata: Vec<u8>,
        }
        impl ProcessRunner for MetadataRunner {
            fn run(&self, command: &ProcessCommand) -> Result<ProcessOutput, String> {
                if command.args.first().is_some_and(|arg| arg == "metadata") {
                    Ok(ProcessOutput {
                        success: true,
                        exit_code: Some(0),
                        stdout: self.metadata.clone(),
                        stderr: Vec::new(),
                    })
                } else {
                    Ok(ProcessOutput {
                        success: true,
                        exit_code: Some(0),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    })
                }
            }

            fn which(&self, _: &str) -> Option<PathBuf> {
                Some(PathBuf::from("/usr/bin/cargo"))
            }
        }

        let selected = determine_rust_artifact(
            &crate_dir,
            false,
            &MetadataRunner {
                metadata: serde_json::to_vec(&metadata).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(selected, artifact);
    }

    #[test]
    fn test_integration_typescript_fixture_build() {
        let fixture_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sdk/typescript/tests/fixture");
        if !fixture_dir.exists() {
            eprintln!(
                "SKIPPED: TypeScript fixture directory not found at {}",
                fixture_dir.display()
            );
            return;
        }

        let runner = OsProcessRunner;
        if runner.which("node").is_none() {
            eprintln!(
                "SKIPPED: TypeScript build fixture integration test skipped: node not found in PATH"
            );
            return;
        }
        if find_local_jco(&fixture_dir).is_none() {
            eprintln!(
                "SKIPPED: TypeScript build fixture integration test skipped: local JCO not found (run npm install in sdk/typescript/tests/fixture)"
            );
            return;
        }

        let res = build_extension(&fixture_dir, false, &runner);
        if !res.success {
            panic!(
                "TypeScript fixture build failed:\n{}",
                res.diagnostics.join("\n")
            );
        }
        assert!(res.success);
        assert_eq!(res.extension_id.as_deref(), Some("org.shilpo.ts-fixture"));
        let wasm_path = fixture_dir.join("extension.wasm");
        assert!(wasm_path.is_file());
        let bytes = fs::read(&wasm_path).unwrap();
        assert!(bytes.len() > 1000);
        assert_eq!(&bytes[0..4], b"\x00asm");
    }

    #[test]
    fn test_integration_rust_fixture_build() {
        let fixture_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bar-menu-component");
        if !fixture_dir.exists() {
            eprintln!(
                "SKIPPED: Rust fixture directory not found at {}",
                fixture_dir.display()
            );
            return;
        }

        let runner = OsProcessRunner;
        if runner.which("cargo").is_none() {
            eprintln!("SKIPPED: Rust fixture build test skipped: cargo not found in PATH");
            return;
        }
        if runner.which("cargo-component").is_none() {
            eprintln!(
                "SKIPPED: Rust fixture build test skipped: cargo-component not found in PATH"
            );
            return;
        }

        let res = build_extension(&fixture_dir, false, &runner);
        if !res.success {
            panic!("Rust fixture build failed:\n{}", res.diagnostics.join("\n"));
        }
        assert!(res.success);
        assert_eq!(
            res.extension_id.as_deref(),
            Some("org.shilpo.bar-menu-fixture")
        );
        let wasm_path = fixture_dir.join("extension.wasm");
        assert!(wasm_path.is_file());
    }
}
