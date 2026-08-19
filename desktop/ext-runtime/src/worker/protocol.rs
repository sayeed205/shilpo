use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use shilpo_ext_api::{CanonicalId, ExtensionId, ViewTree};

use crate::effects::AuthorizedHostOperation;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContributionSurface {
    Bar,
    BarMenu,
    Desktop,
    Settings,
    SidePanel,
    Action,
    Background,
    Shortcut,
    #[serde(rename = "wallpaper")]
    Wallpaper,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper_modes: Option<Vec<shilpo_ext_api::WallpaperMode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper_targets: Option<Vec<shilpo_ext_api::WallpaperTargetKind>>,
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

pub use crate::circuit_breaker::{
    CircuitNotice, CircuitNoticeKind, CircuitStateKind, WasmExtensionStatus,
};

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
    #[serde(default)]
    pub wasm_extensions: Arc<[WasmExtensionStatus]>,
    #[serde(default)]
    pub dev_overrides: Arc<[ExtensionId]>,
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
    DevReload {
        expected_host_gen: super::process::HostGeneration,
        session_id: String,
        extension_id: ExtensionId,
        canonical_root: PathBuf,
        artifact_path: PathBuf,
        build_sequence: u64,
    },
    DevUnload {
        expected_host_gen: super::process::HostGeneration,
        session_id: String,
        extension_id: ExtensionId,
    },
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DevReloadOutcome {
    pub session_id: String,
    pub build_sequence: u64,
    pub outcome: String,
    pub engine_generation: ExtensionGeneration,
    pub diagnostic_code: String,
    pub message: String,
    pub update: Option<ExtensionUpdate>,
}

impl DevReloadOutcome {
    pub fn applied(
        session_id: impl Into<String>,
        build_sequence: u64,
        engine_generation: ExtensionGeneration,
        message: impl Into<String>,
        update: Option<ExtensionUpdate>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            build_sequence,
            outcome: "applied".into(),
            engine_generation,
            diagnostic_code: "OK".into(),
            message: message.into(),
            update,
        }
    }

    pub fn rejected(
        session_id: impl Into<String>,
        build_sequence: u64,
        engine_generation: ExtensionGeneration,
        diagnostic_code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            build_sequence,
            outcome: "rejected".into(),
            engine_generation,
            diagnostic_code: diagnostic_code.into(),
            message: message.into(),
            update: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionUpdate {
    #[serde(default)]
    pub host_generation: super::process::HostGeneration,
    pub generation: ExtensionGeneration,
    pub snapshot: Option<ExtensionSnapshot>,
    pub effects: Vec<(ExtensionId, AuthorizedHostOperation)>,
    pub invalidated_views: Vec<CanonicalId>,
    #[serde(default)]
    pub circuit_notices: Vec<CircuitNotice>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contribution_surface_wallpaper_serialization_round_trip() {
        let surface = ContributionSurface::Wallpaper;
        let json = serde_json::to_string(&surface)
            .expect("ContributionSurface::Wallpaper should serialize");
        assert_eq!(json, "\"wallpaper\"");

        let deserialized: ContributionSurface =
            serde_json::from_str(&json).expect("ContributionSurface::Wallpaper should deserialize");
        assert_eq!(deserialized, ContributionSurface::Wallpaper);

        // Legacy surface names must be rejected.
        let legacy_json = "\"Launcher\"";
        let legacy_result: Result<ContributionSurface, _> = serde_json::from_str(legacy_json);
        assert!(
            legacy_result.is_err(),
            "Legacy 'Launcher' surface must fail deserialization"
        );
    }

    #[test]
    fn test_circuit_state_protocol_round_trips_for_every_state() {
        let states = [
            (
                WasmExtensionStatus {
                    id: ExtensionId::new("io.github.test.healthy").unwrap(),
                    runtime_kind: ExtensionRuntimeKind::Wasm,
                    state: CircuitStateKind::Closed,
                    consecutive_failures: Some(0),
                    consecutive_successes: None,
                    trip_count: 0,
                    retry_after_ms: None,
                    latest_diagnostic: None,
                },
                r#"{"id":"io.github.test.healthy","runtime_kind":"wasm","state":"closed","consecutive_failures":0,"trip_count":0}"#,
            ),
            (
                WasmExtensionStatus {
                    id: ExtensionId::new("io.github.test.open").unwrap(),
                    runtime_kind: ExtensionRuntimeKind::Wasm,
                    state: CircuitStateKind::Open,
                    consecutive_failures: None,
                    consecutive_successes: None,
                    trip_count: 2,
                    retry_after_ms: Some(45000),
                    latest_diagnostic: Some("runtime trap: panic".into()),
                },
                r#"{"id":"io.github.test.open","runtime_kind":"wasm","state":"open","trip_count":2,"retry_after_ms":45000,"latest_diagnostic":"runtime trap: panic"}"#,
            ),
            (
                WasmExtensionStatus {
                    id: ExtensionId::new("io.github.test.halfopen").unwrap(),
                    runtime_kind: ExtensionRuntimeKind::Wasm,
                    state: CircuitStateKind::HalfOpen,
                    consecutive_failures: None,
                    consecutive_successes: Some(2),
                    trip_count: 1,
                    retry_after_ms: None,
                    latest_diagnostic: None,
                },
                r#"{"id":"io.github.test.halfopen","runtime_kind":"wasm","state":"half_open","consecutive_successes":2,"trip_count":1}"#,
            ),
            (
                WasmExtensionStatus {
                    id: ExtensionId::new("io.github.test.perm").unwrap(),
                    runtime_kind: ExtensionRuntimeKind::Wasm,
                    state: CircuitStateKind::PermanentlyDisabled,
                    consecutive_failures: None,
                    consecutive_successes: None,
                    trip_count: 4,
                    retry_after_ms: None,
                    latest_diagnostic: Some("4 failed trip cycles".into()),
                },
                r#"{"id":"io.github.test.perm","runtime_kind":"wasm","state":"permanently_disabled","trip_count":4,"latest_diagnostic":"4 failed trip cycles"}"#,
            ),
        ];

        for (status, expected_json) in states {
            let json = serde_json::to_string(&status).expect("should serialize");
            assert_eq!(json, expected_json);

            let deserialized: WasmExtensionStatus =
                serde_json::from_str(&json).expect("should deserialize");
            assert_eq!(deserialized, status);
        }
    }

    #[test]
    fn test_snapshot_and_update_missing_field_backward_tolerance() {
        // Old snapshot json without wasm_extensions
        let old_snapshot_json = r#"{
            "generation": 42,
            "descriptors": [],
            "views": {},
            "diagnostics": [],
            "settings_schemas": {},
            "prevalidated_asset_roots": {},
            "script_extensions": []
        }"#;

        let snapshot: ExtensionSnapshot = serde_json::from_str(old_snapshot_json)
            .expect("should deserialize with default wasm_extensions");
        assert_eq!(snapshot.generation, ExtensionGeneration(42));
        assert!(snapshot.wasm_extensions.is_empty());

        // Old update json without circuit_notices
        let old_update_json = r#"{
            "host_generation": 1,
            "generation": 42,
            "effects": [],
            "invalidated_views": []
        }"#;

        let update: ExtensionUpdate = serde_json::from_str(old_update_json)
            .expect("should deserialize with default circuit_notices");
        assert_eq!(update.generation, ExtensionGeneration(42));
        assert!(update.circuit_notices.is_empty());
    }
}
