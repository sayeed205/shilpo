use std::fmt;

/// Structured error types for shell runtime operations.
#[derive(Debug)]
pub enum ShellError {
    WindowCreation {
        surface: &'static str,
        message: String,
    },
    ActionFailed(String),
    Service(shilpo_services::ServiceError),
    Config(crate::config::ConfigError),
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WindowCreation { surface, message } => {
                write!(f, "Window creation failed [{surface}]: {message}")
            }
            Self::ActionFailed(msg) => write!(f, "Shell action failed: {msg}"),
            Self::Service(err) => write!(f, "Shell service error: {err}"),
            Self::Config(err) => write!(f, "Shell config error: {err}"),
        }
    }
}

impl std::error::Error for ShellError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Service(err) => Some(err),
            Self::Config(err) => Some(err),
            Self::WindowCreation { .. } | Self::ActionFailed(..) => None,
        }
    }
}

impl From<shilpo_services::ServiceError> for ShellError {
    fn from(err: shilpo_services::ServiceError) -> Self {
        Self::Service(err)
    }
}

impl From<crate::config::ConfigError> for ShellError {
    fn from(err: crate::config::ConfigError) -> Self {
        Self::Config(err)
    }
}
