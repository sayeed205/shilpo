use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::manifest::WallpaperMode;

#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    OutputsChanged,
    ThemeChanged,
    PaletteGenerated,
    WallpaperChanged,
    NetworkChanged,
    MediaChanged,
    PowerChanged,
    TimerFired,
    WorkspaceChanged,
}

#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum BarMenuCloseReason {
    SourceToggle,
    Escape,
    FocusLost,
    OutsideClick,
    OverviewOpened,
    BarClosed,
    DisplayRemoved,
    OwnerRemoved,
    SourceUnavailable,
}

#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperRequestReason {
    Activate,
    UserNext,
    SlideshowTick,
    WorkspaceChanged,
    SettingsChanged,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceTarget {
    pub workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WallpaperTarget {
    Global,
    Workspace(WorkspaceTarget),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WallpaperRequest {
    pub request_id: String,
    pub contribution_id: String,
    pub reason: WallpaperRequestReason,
    pub mode: WallpaperMode,
    pub target: WallpaperTarget,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionEvent {
    ShellStarted,
    ShellStopping,
    OutputsChanged,
    ThemeChanged {
        mode: String,
    },
    PaletteGenerated {
        accent: String,
    },
    WallpaperChanged {
        path: String,
    },
    NetworkChanged {
        connected: bool,
    },
    MediaChanged {
        title: Option<String>,
        artist: Option<String>,
        playing: bool,
    },
    PowerChanged {
        percentage: Option<f32>,
        charging: bool,
    },
    TimerFired {
        name: String,
    },
    WorkspaceChanged {
        workspace_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_name: Option<String>,
    },
    WallpaperRequest(WallpaperRequest),
    WallpaperResult {
        request_id: String,
        success: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    ContributionMounted {
        contribution_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instance_id: Option<String>,
        width: f32,
        height: f32,
    },
    ContributionUnmounted {
        contribution_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instance_id: Option<String>,
    },
    ContributionResized {
        contribution_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instance_id: Option<String>,
        width: f32,
        height: f32,
    },
    ContributionSettingsChanged {
        contribution_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instance_id: Option<String>,
        settings: serde_json::Value,
    },
    BarMenuOpened {
        contribution_id: String,
        instance_id: String,
    },
    BarMenuClosed {
        contribution_id: String,
        instance_id: String,
        reason: BarMenuCloseReason,
    },
    Input {
        contribution_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instance_id: Option<String>,
        event_id: String,
        value: Option<serde_json::Value>,
    },
    StateValue {
        watch_id: u64,
        key: String,
        value: Option<serde_json::Value>,
        revision: u64,
    },
    HttpResponse {
        request_id: String,
        status: Option<u16>,
        body: String,
        error: Option<String>,
    },
    LocationResponse {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        latitude: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        longitude: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accuracy_meters: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

impl ExtensionEvent {
    pub fn subscription_kind(&self) -> Option<EventKind> {
        match self {
            Self::ShellStarted
            | Self::ShellStopping
            | Self::WallpaperRequest(_)
            | Self::WallpaperResult { .. }
            | Self::ContributionMounted { .. }
            | Self::ContributionUnmounted { .. }
            | Self::ContributionResized { .. }
            | Self::ContributionSettingsChanged { .. }
            | Self::BarMenuOpened { .. }
            | Self::BarMenuClosed { .. }
            | Self::Input { .. }
            | Self::StateValue { .. }
            | Self::HttpResponse { .. }
            | Self::LocationResponse { .. } => None,
            Self::OutputsChanged => Some(EventKind::OutputsChanged),
            Self::ThemeChanged { .. } => Some(EventKind::ThemeChanged),
            Self::PaletteGenerated { .. } => Some(EventKind::PaletteGenerated),
            Self::WallpaperChanged { .. } => Some(EventKind::WallpaperChanged),
            Self::NetworkChanged { .. } => Some(EventKind::NetworkChanged),
            Self::MediaChanged { .. } => Some(EventKind::MediaChanged),
            Self::PowerChanged { .. } => Some(EventKind::PowerChanged),
            Self::TimerFired { .. } => Some(EventKind::TimerFired),
            Self::WorkspaceChanged { .. } => Some(EventKind::WorkspaceChanged),
        }
    }
}
