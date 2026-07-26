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
        #[serde(default)]
        request_id: String,
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

#[cfg(test)]
mod tests {
    use super::HostEffect;

    #[test]
    fn legacy_http_requests_default_the_correlation_id() {
        let effect: HostEffect = serde_json::from_value(serde_json::json!({
            "kind": "http_request",
            "url": "https://example.com/weather",
            "method": "GET"
        }))
        .unwrap();

        assert!(matches!(
            effect,
            HostEffect::HttpRequest { request_id, .. } if request_id.is_empty()
        ));
    }
}
