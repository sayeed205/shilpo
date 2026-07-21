use serde::{Deserialize, Serialize};

/// Manifest defining a 3rd-party or user extension for Shilpo Shell.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

pub trait ShellExtension: Send + Sync {
    fn manifest(&self) -> &ExtensionManifest;
}
