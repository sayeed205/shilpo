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
    LocationRead,
}

/// Crate-private parsed and normalized HTTP target.
#[derive(Clone, Debug)]
pub(crate) struct CanonicalHttpTarget {
    url: url::Url,
}

impl CanonicalHttpTarget {
    /// Parse and validate an HTTP request target from raw URL and method strings.
    pub(crate) fn parse(raw_url: &str, method: &str) -> Option<Self> {
        if method != "GET" {
            return None;
        }
        let url = url::Url::parse(raw_url).ok()?;

        if url.scheme() != "https" {
            return None;
        }
        if url.host_str().is_none() || url.host_str() == Some("") {
            return None;
        }
        if !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        if url.fragment().is_some() {
            return None;
        }

        Some(Self { url })
    }

    pub(crate) fn host(&self) -> &str {
        self.url.host_str().unwrap_or("")
    }

    pub(crate) fn path(&self) -> &str {
        self.url.path()
    }

    pub(crate) fn into_url(self) -> url::Url {
        self.url
    }
}

/// An authorized HTTP request token containing a validated URL.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizedHttpRequest {
    pub(crate) request_id: String,
    pub(crate) url: url::Url,
}

impl AuthorizedHttpRequest {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn url(&self) -> &url::Url {
        &self.url
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AuthorizedHostEffectKind {
    NonHttp(HostEffect),
    HttpRequest(AuthorizedHttpRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthorizedHostEffect(pub(crate) AuthorizedHostEffectKind);

impl AuthorizedHostEffect {
    pub(crate) fn non_http(effect: HostEffect) -> Result<Self, HostEffect> {
        if matches!(effect, HostEffect::HttpRequest { .. }) {
            Err(effect)
        } else {
            Ok(Self(AuthorizedHostEffectKind::NonHttp(effect)))
        }
    }

    pub(crate) fn http_request(request_id: String, target: CanonicalHttpTarget) -> Self {
        Self(AuthorizedHostEffectKind::HttpRequest(
            AuthorizedHttpRequest {
                request_id,
                url: target.into_url(),
            },
        ))
    }

    pub fn into_kind(self) -> AuthorizedHostEffectKind {
        self.0
    }

    pub fn kind(&self) -> &AuthorizedHostEffectKind {
        &self.0
    }
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
