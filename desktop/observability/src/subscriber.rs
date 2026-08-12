use crate::{ProcessRole, is_profile_enabled, paths};
use std::{
    fs, io,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

/// Guard object managing subscriber lifecycle and trace file finalization.
pub struct ObservabilityGuard {
    inner: Option<GuardInner>,
}

struct GuardInner {
    flush_guard: tracing_chrome::FlushGuard,
    active_path: PathBuf,
    final_path: PathBuf,
}

impl ObservabilityGuard {
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    pub fn enabled(
        flush_guard: tracing_chrome::FlushGuard,
        active_path: PathBuf,
        final_path: PathBuf,
    ) -> Self {
        Self {
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
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            drop(inner.flush_guard);
            if let Err(error) = fs::rename(&inner.active_path, &inner.final_path) {
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

    let filter = tracing_subscriber::EnvFilter::builder().parse_lossy(filter_str);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(io::stderr);

    if !is_profile_enabled() {
        let subscriber = tracing_subscriber::registry().with(filter).with(fmt_layer);
        if subscriber.try_init().is_err() {
            return Err(ObservabilityError::AlreadyInitialized);
        }
        SUBSCRIBER_INITIALIZED.store(true, Ordering::SeqCst);
        return Ok(ObservabilityGuard::disabled());
    }

    let profile_dir = paths::resolve_profile_dir()?;
    if let Err(source) = fs::create_dir_all(&profile_dir) {
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
        .with(filter)
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
        flush_guard,
        active_path,
        final_path,
    ))
}

/// Reset subscriber initialization state (FOR UNIT TESTS ONLY).
#[doc(hidden)]
pub fn reset_initialized_for_testing() {
    SUBSCRIBER_INITIALIZED.store(false, Ordering::SeqCst);
}
