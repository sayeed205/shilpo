use crate::events::WallpaperTarget;
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
pub enum HostOperation {
    ShowNotification {
        title: String,
        body: String,
        icon: Option<String>,
    },
    InvokeAction {
        action_id: String,
        payload_json: Option<String>,
    },
    SetWallpaper {
        path: String,
        source: WallpaperSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<WallpaperTarget>,
    },
    SetThemeSource {
        color: String,
    },
    ClipboardWrite {
        text: String,
    },
    HttpRequest {
        #[serde(default)]
        request_id: String,
        url: String,
        method: String,
    },
    LocationRead,
}
