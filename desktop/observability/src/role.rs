use std::fmt;

use serde::{Deserialize, Serialize};

/// Typed process role identifier for Shilpo durable components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessRole {
    Shell,
    Settings,
    ExtensionHost,
    DeviceDaemon,
    ThemeDaemon,
}

impl ProcessRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::Settings => "settings",
            Self::ExtensionHost => "extension-host",
            Self::DeviceDaemon => "device-daemon",
            Self::ThemeDaemon => "theme-daemon",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "shell" => Some(Self::Shell),
            "settings" => Some(Self::Settings),
            "extension-host" => Some(Self::ExtensionHost),
            "device-daemon" => Some(Self::DeviceDaemon),
            "theme-daemon" => Some(Self::ThemeDaemon),
            _ => None,
        }
    }
}

impl fmt::Display for ProcessRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
