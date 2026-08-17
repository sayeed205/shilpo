use std::{
    fs, io,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use tracing_subscriber::{
    EnvFilter, Registry, layer::SubscriberExt, reload::Handle, util::SubscriberInitExt,
};

use crate::{ProcessRole, is_profile_enabled, paths};

static SUBSCRIBER_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error("observability subscriber has already been initialized in this process")]
    AlreadyInitialized,
    #[error("relative profile directory '{0}' is invalid; SHILPO_PROFILE_DIR must be absolute")]
    InvalidProfileDir(PathBuf),
    #[error("failed to create profile directory '{path}': {source}")]
    CreateDirFailed { path: PathBuf, source: io::Error },
    #[error("failed to create trace file '{path}': {source}")]
    CreateFileFailed { path: PathBuf, source: io::Error },
    #[error("failed to finalize trace file '{path}': {source}")]
    FinalizeFailed { path: PathBuf, source: io::Error },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FilterError {
    #[error("filter directive cannot be empty or whitespace")]
    EmptyFilter,
    #[error("invalid filter directive '{0}'")]
    InvalidFilter(String),
    #[error("failed to reload filter: {0}")]
    ReloadFailed(String),
}

/// Controller for dynamically querying and modifying the active tracing `EnvFilter`.
#[derive(Clone)]
pub struct LogFilterController {
    inner: Arc<LogFilterControllerInner>,
}

struct LogFilterControllerInner {
    handle: Handle<EnvFilter, Registry>,
    current_filter: Mutex<String>,
    _layer_keepalive: Option<Arc<dyn std::any::Any + Send + Sync>>,
}

impl std::fmt::Debug for LogFilterController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogFilterController")
            .field("current_filter", &self.current_filter())
            .finish()
    }
}

impl LogFilterController {
    pub(crate) fn new(handle: Handle<EnvFilter, Registry>, initial_filter: String) -> Self {
        Self {
            inner: Arc::new(LogFilterControllerInner {
                handle,
                current_filter: Mutex::new(initial_filter),
                _layer_keepalive: None,
            }),
        }
    }

    #[doc(hidden)]
    pub fn new_for_testing(initial: &str) -> Self {
        let filter = tracing_subscriber::EnvFilter::builder().parse_lossy(initial);
        let (reload_layer, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
        Self {
            inner: Arc::new(LogFilterControllerInner {
                handle: reload_handle,
                current_filter: Mutex::new(initial.to_string()),
                _layer_keepalive: Some(Arc::new(reload_layer)),
            }),
        }
    }

    pub fn current_filter(&self) -> String {
        self.inner.current_filter.lock().unwrap().clone()
    }

    pub fn set_filter(&self, filter_str: &str) -> Result<(), FilterError> {
        let trimmed = filter_str.trim();
        if trimmed.is_empty() {
            return Err(FilterError::EmptyFilter);
        }

        let new_filter = trimmed
            .parse::<EnvFilter>()
            .map_err(|err| FilterError::InvalidFilter(err.to_string()))?;

        let mut current = self.inner.current_filter.lock().unwrap();

        self.inner
            .handle
            .reload(new_filter)
            .map_err(|err| FilterError::ReloadFailed(err.to_string()))?;

        *current = trimmed.to_string();
        Ok(())
    }
}

/// Guard object managing subscriber lifecycle and trace file finalization.
pub struct ObservabilityGuard {
    controller: Option<LogFilterController>,
    inner: Option<GuardInner>,
}

struct GuardInner {
    flush_guard: tracing_chrome::FlushGuard,
    active_path: PathBuf,
    final_path: PathBuf,
}

impl ObservabilityGuard {
    pub fn disabled() -> Self {
        Self {
            controller: None,
            inner: None,
        }
    }

    pub fn disabled_with_controller(controller: LogFilterController) -> Self {
        Self {
            controller: Some(controller),
            inner: None,
        }
    }

    pub fn enabled(
        controller: LogFilterController,
        flush_guard: tracing_chrome::FlushGuard,
        active_path: PathBuf,
        final_path: PathBuf,
    ) -> Self {
        Self {
            controller: Some(controller),
            inner: Some(GuardInner {
                flush_guard,
                active_path,
                final_path,
            }),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    pub fn log_filter_controller(&self) -> Option<LogFilterController> {
        self.controller.clone()
    }
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            drop(inner.flush_guard);
            if inner.final_path.exists() {
                eprintln!(
                    "observability warning: refusing to overwrite existing trace '{}'",
                    inner.final_path.display()
                );
            } else if let Err(error) = fs::rename(&inner.active_path, &inner.final_path) {
                eprintln!(
                    "observability warning: failed to finalize trace '{}': {error}",
                    inner.active_path.display()
                );
            }
        }
    }
}

/// Initialize tracing subscriber for a durable Shilpo process role.
pub fn init(
    role: ProcessRole,
    default_filter: &str,
) -> Result<ObservabilityGuard, ObservabilityError> {
    if SUBSCRIBER_INITIALIZED.load(Ordering::SeqCst) {
        return Err(ObservabilityError::AlreadyInitialized);
    }

    let filter_str = std::env::var("RUST_LOG")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default_filter.to_string());

    let filter = tracing_subscriber::EnvFilter::builder().parse_lossy(&filter_str);
    let (reload_layer, reload_handle) = tracing_subscriber::reload::Layer::new(filter);
    let controller = LogFilterController::new(reload_handle, filter_str.clone());

    let fmt_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(io::stderr);

    if !is_profile_enabled() {
        let subscriber = tracing_subscriber::registry()
            .with(reload_layer)
            .with(fmt_layer);
        if subscriber.try_init().is_err() {
            return Err(ObservabilityError::AlreadyInitialized);
        }
        SUBSCRIBER_INITIALIZED.store(true, Ordering::SeqCst);
        return Ok(ObservabilityGuard::disabled_with_controller(controller));
    }

    let profile_dir = match paths::resolve_profile_dir() {
        Ok(path) => path,
        Err(error) => {
            install_fallback_subscriber(default_filter);
            return Err(error);
        }
    };
    if let Err(source) = fs::create_dir_all(&profile_dir) {
        install_fallback_subscriber(default_filter);
        return Err(ObservabilityError::CreateDirFailed {
            path: profile_dir,
            source,
        });
    }

    let pid = std::process::id();
    let active_filename = paths::generate_active_filename(role, pid);
    let final_filename = paths::active_to_final_filename(&active_filename);
    let active_path = profile_dir.join(&active_filename);
    let final_path = profile_dir.join(&final_filename);

    let writer = match fs::OpenOptions::new()
        .append(true)
        .create_new(true)
        .open(&active_path)
    {
        Ok(file) => file,
        Err(source) => {
            install_fallback_subscriber(default_filter);
            return Err(ObservabilityError::CreateFileFailed {
                path: active_path,
                source,
            });
        }
    };

    let (chrome_layer, flush_guard) = tracing_chrome::ChromeLayerBuilder::new()
        .writer(writer)
        .include_args(true)
        .include_locations(false)
        .build();

    let subscriber = tracing_subscriber::registry()
        .with(reload_layer)
        .with(fmt_layer)
        .with(chrome_layer);

    if subscriber.try_init().is_err() {
        let _ = fs::remove_file(&active_path);
        return Err(ObservabilityError::AlreadyInitialized);
    }
    SUBSCRIBER_INITIALIZED.store(true, Ordering::SeqCst);

    tracing::info!(
        target: "shilpo_profile",
        process_role = role.as_str(),
        os_pid = pid,
        "process_start"
    );

    Ok(ObservabilityGuard::enabled(
        controller,
        flush_guard,
        active_path,
        final_path,
    ))
}

fn install_fallback_subscriber(default_filter: &str) {
    let filter_str = std::env::var("RUST_LOG")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_filter.to_owned());
    let filter = tracing_subscriber::EnvFilter::builder().parse_lossy(filter_str);
    let (reload_layer, _reload_handle) = tracing_subscriber::reload::Layer::new(filter);
    let subscriber = tracing_subscriber::registry().with(reload_layer).with(
        tracing_subscriber::fmt::layer()
            .compact()
            .with_writer(io::stderr),
    );
    if subscriber.try_init().is_ok() {
        SUBSCRIBER_INITIALIZED.store(true, Ordering::SeqCst);
    }
}

/// Reset subscriber initialization state (FOR UNIT TESTS ONLY).
#[doc(hidden)]
pub fn reset_initialized_for_testing() {
    SUBSCRIBER_INITIALIZED.store(false, Ordering::SeqCst);
}
