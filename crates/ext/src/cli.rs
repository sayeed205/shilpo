use crate::adapter::{ExtensionRuntime, RuntimeBudget};
use crate::events::ExtensionEvent;
use crate::manifest::{ExtensionId, ExtensionManifest};
use crate::view::ViewLimits;
use crate::wasm::{WasmModule, WasmRuntime};
use flate2::Compression;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tar::Builder;

const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;

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

    pub fn list(dev_paths: &[PathBuf]) -> Vec<ExtensionCliResult> {
        dev_paths.iter().map(|path| Self::check(path)).collect()
    }
}

pub fn run_cli(args: &[String]) -> i32 {
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return 2;
    };
    let state_dir = default_extension_state_dir();
    let result = match command {
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
        "list" => {
            let results = ExtensionCli::list_dev(&state_dir);
            if results.is_empty() {
                println!("No development extensions registered.");
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

fn inspect_extension(dir: &Path) -> Result<CheckedExtension, (Option<String>, Vec<String>)> {
    let mut diagnostics = Vec::new();
    let manifest_path = dir.join("extension.toml");
    if !is_regular_file(&manifest_path, "manifest", &mut diagnostics) {
        return Err((None, diagnostics));
    }
    let source = fs::read_to_string(&manifest_path).map_err(|error| {
        (
            None,
            vec![format!(
                "error[manifest.read]: failed to read {}: {error}",
                manifest_path.display()
            )],
        )
    })?;
    let manifest = ExtensionManifest::from_toml(&source)
        .map_err(|error| (None, vec![format!("error[manifest.invalid]: {error}")]))?;
    let extension_id = Some(manifest.id.to_string());
    diagnostics.push(format!(
        "info[manifest.valid]: '{}' {}",
        manifest.id, manifest.version
    ));

    let mut files = BTreeSet::new();
    files.insert(PathBuf::from("extension.toml"));
    let mut total_size = file_size(&manifest_path, &mut diagnostics);

    if let Some(library) = &manifest.library {
        let relative = PathBuf::from(&library.path);
        let path = dir.join(&relative);
        if !is_regular_file(&path, "library", &mut diagnostics) {
            return Err((extension_id, diagnostics));
        }
        match fs::read(&path) {
            Ok(bytes) => {
                if let Err(error) = WasmRuntime::validate_module(&bytes) {
                    diagnostics.push(format!("error[wasm.invalid]: {error}"));
                } else {
                    diagnostics.push(format!(
                        "info[wasm.valid]: component interface validated at {}",
                        relative.display()
                    ));
                }
            }
            Err(error) => diagnostics.push(format!(
                "error[wasm.read]: failed to read {}: {error}",
                path.display()
            )),
        }
        total_size += file_size(&path, &mut diagnostics);
        files.insert(relative);
    }

    for page in &manifest.contributions.settings_pages {
        let relative = PathBuf::from(&page.schema);
        let path = dir.join(&relative);
        if !is_regular_file(&path, "settings schema", &mut diagnostics) {
            continue;
        }
        validate_settings_schema(&path, &mut diagnostics);
        total_size += file_size(&path, &mut diagnostics);
        files.insert(relative);
    }

    for optional in ["README.md", "LICENSE"] {
        let relative = PathBuf::from(optional);
        let path = dir.join(&relative);
        if !path.exists() {
            continue;
        }
        if is_regular_file(&path, optional, &mut diagnostics) {
            total_size += file_size(&path, &mut diagnostics);
            files.insert(relative);
        }
    }
    for directory in ["assets", "i18n"] {
        collect_runtime_files(
            dir,
            Path::new(directory),
            &mut files,
            &mut total_size,
            &mut diagnostics,
        );
    }

    if total_size > MAX_PACKAGE_BYTES {
        diagnostics.push(format!(
            "error[package.size]: runtime files use {total_size} bytes; limit is {MAX_PACKAGE_BYTES}"
        ));
    }
    let has_error = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.starts_with("error["));
    if has_error {
        Err((extension_id, diagnostics))
    } else {
        Ok(CheckedExtension {
            manifest,
            files: files.into_iter().collect(),
            diagnostics,
        })
    }
}

fn probe_runtime(dir: &Path, manifest: &ExtensionManifest) -> Result<(), String> {
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

fn validate_settings_schema(path: &Path, diagnostics: &mut Vec<String>) {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            diagnostics.push(format!(
                "error[settings.read]: failed to read {}: {error}",
                path.display()
            ));
            return;
        }
    };
    let schema: Value = match serde_json::from_str(&source) {
        Ok(schema) => schema,
        Err(error) => {
            diagnostics.push(format!(
                "error[settings.json]: {} is invalid JSON: {error}",
                path.display()
            ));
            return;
        }
    };
    if let Err(error) = jsonschema::meta::validate(&schema) {
        diagnostics.push(format!(
            "error[settings.schema]: {} is not valid JSON Schema: {error}",
            path.display()
        ));
        return;
    }
    if contains_remote_reference(&schema) {
        diagnostics.push(format!(
            "error[settings.reference]: {} contains a remote $ref",
            path.display()
        ));
        return;
    }
    let defaults = settings_defaults(&schema);
    match jsonschema::validator_for(&schema) {
        Ok(validator) => {
            if let Err(error) = validator.validate(&defaults) {
                diagnostics.push(format!(
                    "error[settings.defaults]: defaults in {} are invalid: {error}",
                    path.display()
                ));
            } else {
                diagnostics.push(format!(
                    "info[settings.valid]: schema and defaults validated at {}",
                    path.display()
                ));
            }
        }
        Err(error) => diagnostics.push(format!(
            "error[settings.compile]: failed to compile {}: {error}",
            path.display()
        )),
    }
}

fn settings_defaults(schema: &Value) -> Value {
    let mut defaults = Map::new();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (key, property) in properties {
            if let Some(value) = property.get("default") {
                defaults.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(defaults)
}

fn contains_remote_reference(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            (key == "$ref"
                && value
                    .as_str()
                    .is_some_and(|reference| reference.contains("://")))
                || contains_remote_reference(value)
        }),
        Value::Array(values) => values.iter().any(contains_remote_reference),
        _ => false,
    }
}

fn collect_runtime_files(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
    total_size: &mut u64,
    diagnostics: &mut Vec<String>,
) {
    let path = root.join(relative);
    if !path.exists() {
        return;
    }
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            diagnostics.push(format!(
                "error[asset.metadata]: failed to inspect {}: {error}",
                path.display()
            ));
            return;
        }
    };
    if metadata.file_type().is_symlink() {
        diagnostics.push(format!(
            "error[asset.symlink]: symbolic links are not packageable: {}",
            path.display()
        ));
        return;
    }
    if metadata.is_file() {
        *total_size += metadata.len();
        if metadata.len() > MAX_FILE_BYTES {
            diagnostics.push(format!(
                "error[asset.size]: {} exceeds the per-file limit",
                path.display()
            ));
        }
        files.insert(relative.to_path_buf());
        return;
    }
    if !metadata.is_dir() {
        return;
    }
    let entries = match fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(format!(
                "error[asset.read]: failed to read {}: {error}",
                path.display()
            ));
            return;
        }
    };
    for entry in entries.flatten() {
        collect_runtime_files(
            root,
            &relative.join(entry.file_name()),
            files,
            total_size,
            diagnostics,
        );
    }
}

fn is_regular_file(path: &Path, label: &str, diagnostics: &mut Vec<String>) -> bool {
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

fn file_size(path: &Path, diagnostics: &mut Vec<String>) -> u64 {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.len() > MAX_FILE_BYTES {
                diagnostics.push(format!(
                    "error[file.size]: {} exceeds the per-file limit",
                    path.display()
                ));
            }
            metadata.len()
        }
        Err(_) => 0,
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

fn follow_log(id: &ExtensionId, state_dir: &Path) {
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
        "Usage: shilpo ext <check|pack|dev|reload|logs|list> [path-or-id] [--output DIR] [--follow]"
    );
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
