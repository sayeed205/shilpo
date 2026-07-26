use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperSource {
    ExtensionAsset,
    LocalFile,
    Remote,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostEffect {
    InvalidateView {
        contribution_id: String,
    },
    ShowNotification {
        title: String,
        body: String,
        icon: Option<String>,
    },
    InvokeAction {
        action_id: String,
        payload: Option<serde_json::Value>,
    },
    SetWallpaper {
        path: String,
        source: WallpaperSource,
    },
    WallpaperMetadataRead,
    ThemeRead,
    SetThemeSource {
        color: String,
    },
    ClipboardRead,
    ClipboardWrite {
        text: String,
    },
    StateRead {
        key: String,
    },
    StateWrite {
        key: String,
        value: serde_json::Value,
    },
    HttpRequest {
        url: String,
        method: String,
    },
    ExecProcess {
        command: String,
        args: Vec<String>,
    },
    ReadFile {
        path: String,
    },
    WriteFile {
        path: String,
        contents: Vec<u8>,
    },
}
