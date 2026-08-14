use crate::effects::AuthorizedHostOperation;
use serde::{Deserialize, Serialize};
use shilpo_ext_api::{CanonicalId, ExtensionId, ViewTree};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContributionSurface {
    Bar,
    BarMenu,
    Desktop,
    Settings,
    SidePanel,
    #[serde(rename = "search")]
    Search,
    Action,
    Background,
    Shortcut,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionRuntimeKind {
    #[default]
    Wasm,
    TrustedLocalScript,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptExtensionStatus {
    pub id: ExtensionId,
    pub name: String,
    pub version: String,
    pub source: String,
    pub status: String,
    pub contributions_count: usize,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionDescriptor {
    pub id: CanonicalId,
    pub extension_name: String,
    pub name: String,
    pub surface: ContributionSurface,
    #[serde(default)]
    pub runtime_kind: ExtensionRuntimeKind,
    pub settings_schema: Option<String>,
    pub default_size: Option<(u32, u32)>,
    pub minimum_size: Option<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_widget: Option<CanonicalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<CanonicalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_binding: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContributionInstance {
    pub id: String,
    pub contribution: CanonicalId,
    pub output: Option<String>,
    pub width: f32,
    pub height: f32,
    pub settings: serde_json::Value,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ExtensionGeneration(pub u64);

impl ExtensionGeneration {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExtensionSnapshot {
    pub generation: ExtensionGeneration,
    pub descriptors: Arc<[ContributionDescriptor]>,
    pub views: Arc<BTreeMap<CanonicalId, ViewTree>>,
    pub diagnostics: Arc<[String]>,
    pub catalog_changed_at: Option<ExtensionGeneration>,
    pub settings_schemas: Arc<BTreeMap<CanonicalId, serde_json::Value>>,
    pub prevalidated_asset_roots: Arc<BTreeMap<ExtensionId, PathBuf>>,
    #[serde(default)]
    pub script_extensions: Arc<[ScriptExtensionStatus]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtensionChanges {
    pub effects: Vec<(ExtensionId, AuthorizedHostOperation)>,
    pub invalidated_views: Vec<CanonicalId>,
    pub catalog_changed: bool,
}

impl ExtensionChanges {
    pub fn merge(&mut self, mut other: Self) {
        self.effects.append(&mut other.effects);
        self.invalidated_views.append(&mut other.invalidated_views);
        self.catalog_changed |= other.catalog_changed;
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReplaceableEvent {
    Power {
        percentage: Option<f32>,
        charging: bool,
    },
    Network {
        connected: bool,
    },
    Media {
        title: Option<String>,
        artist: Option<String>,
        playing: bool,
    },
    TimerFired(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExtensionCommand {
    Lifecycle {
        expected: ExtensionGeneration,
        event: shilpo_ext_api::ExtensionEvent,
    },
    Input {
        expected: ExtensionGeneration,
        contribution: CanonicalId,
        instance_id: Option<String>,
        event_id: String,
        value: Option<serde_json::Value>,
    },
    Response {
        expected: ExtensionGeneration,
        extension_id: ExtensionId,
        event: shilpo_ext_api::ExtensionEvent,
    },
    Replaceable(ReplaceableEvent),
    ReconcileInstances {
        expected: ExtensionGeneration,
        desired: Vec<ContributionInstance>,
    },
    SourcesChanged,
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionUpdate {
    #[serde(default)]
    pub host_generation: super::process::HostGeneration,
    pub generation: ExtensionGeneration,
    pub snapshot: Option<ExtensionSnapshot>,
    pub effects: Vec<(ExtensionId, AuthorizedHostOperation)>,
    pub invalidated_views: Vec<CanonicalId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contribution_surface_search_serialization_round_trip() {
        let surface = ContributionSurface::Search;
        let json =
            serde_json::to_string(&surface).expect("ContributionSurface::Search should serialize");
        assert_eq!(json, "\"search\"");

        let deserialized: ContributionSurface =
            serde_json::from_str(&json).expect("ContributionSurface::Search should deserialize");
        assert_eq!(deserialized, ContributionSurface::Search);

        // Legacy surface names must be rejected.
        let legacy_json = "\"Launcher\"";
        let legacy_result: Result<ContributionSurface, _> = serde_json::from_str(legacy_json);
        assert!(
            legacy_result.is_err(),
            "Legacy 'Launcher' surface must fail deserialization"
        );
    }
}
