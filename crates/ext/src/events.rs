use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    Input {
        contribution_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instance_id: Option<String>,
        event_id: String,
        value: Option<serde_json::Value>,
    },
    StateValue {
        key: String,
        value: Option<serde_json::Value>,
    },
    HttpResponse {
        request_id: String,
        status: Option<u16>,
        body: String,
        error: Option<String>,
    },
}

impl ExtensionEvent {
    pub fn subscription_kind(&self) -> Option<EventKind> {
        match self {
            Self::ShellStarted
            | Self::ShellStopping
            | Self::ContributionMounted { .. }
            | Self::ContributionUnmounted { .. }
            | Self::ContributionResized { .. }
            | Self::ContributionSettingsChanged { .. }
            | Self::Input { .. }
            | Self::StateValue { .. }
            | Self::HttpResponse { .. } => None,
            Self::OutputsChanged => Some(EventKind::OutputsChanged),
            Self::ThemeChanged { .. } => Some(EventKind::ThemeChanged),
            Self::PaletteGenerated { .. } => Some(EventKind::PaletteGenerated),
            Self::WallpaperChanged { .. } => Some(EventKind::WallpaperChanged),
            Self::NetworkChanged { .. } => Some(EventKind::NetworkChanged),
            Self::MediaChanged { .. } => Some(EventKind::MediaChanged),
            Self::PowerChanged { .. } => Some(EventKind::PowerChanged),
            Self::TimerFired { .. } => Some(EventKind::TimerFired),
        }
    }
}
