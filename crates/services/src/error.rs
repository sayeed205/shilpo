use std::fmt;

/// Structured error types for background desktop services.
#[derive(Debug)]
pub enum ServiceError {
    Compositor {
        component: &'static str,
        message: String,
    },
    Audio {
        backend: &'static str,
        message: String,
    },
    Network {
        message: String,
    },
    Battery {
        message: String,
    },
    AppScanner {
        message: String,
    },
    Ipc {
        context: &'static str,
        message: String,
    },
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compositor { component, message } => {
                write!(f, "Compositor service error [{component}]: {message}")
            }
            Self::Audio { backend, message } => {
                write!(f, "Audio service error [{backend}]: {message}")
            }
            Self::Network { message } => write!(f, "Network service error: {message}"),
            Self::Battery { message } => write!(f, "Battery service error: {message}"),
            Self::AppScanner { message } => write!(f, "Application scanner error: {message}"),
            Self::Ipc { context, message } => write!(f, "IPC error [{context}]: {message}"),
        }
    }
}

impl std::error::Error for ServiceError {}
