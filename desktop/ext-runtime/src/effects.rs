use serde::{Deserialize, Serialize};
use shilpo_ext_api::{Capability, HostOperation, wildcard_matches};

/// Crate-private parsed and normalized HTTP target.
#[derive(Clone, Debug)]
pub struct CanonicalHttpTarget {
    url: url::Url,
}

impl CanonicalHttpTarget {
    /// Parse and validate an HTTP request target from raw URL and method strings.
    pub fn parse(raw_url: &str, method: &str) -> Option<Self> {
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

    pub fn host(&self) -> &str {
        self.url.host_str().unwrap_or("")
    }

    pub fn path(&self) -> &str {
        self.url.path()
    }

    pub fn into_url(self) -> url::Url {
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
pub enum AuthorizedHostOperationKind {
    NonHttp(HostOperation),
    HttpRequest(AuthorizedHttpRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthorizedHostOperation(pub(crate) AuthorizedHostOperationKind);

impl AuthorizedHostOperation {
    pub fn non_http(op: HostOperation) -> Result<Self, HostOperation> {
        if matches!(op, HostOperation::HttpRequest { .. }) {
            Err(op)
        } else {
            Ok(Self(AuthorizedHostOperationKind::NonHttp(op)))
        }
    }

    pub fn http_request(request_id: String, target: CanonicalHttpTarget) -> Self {
        Self(AuthorizedHostOperationKind::HttpRequest(
            AuthorizedHttpRequest {
                request_id,
                url: target.into_url(),
            },
        ))
    }

    pub fn into_kind(self) -> AuthorizedHostOperationKind {
        self.0
    }

    pub fn kind(&self) -> &AuthorizedHostOperationKind {
        &self.0
    }
}

pub fn capability_allows_operation(capability: &Capability, operation: &HostOperation) -> bool {
    match (capability, operation) {
        (Capability::NotificationsShow, HostOperation::ShowNotification { .. }) => true,
        (Capability::ThemeSetSource, HostOperation::SetThemeSource { .. }) => true,
        (Capability::ClipboardWrite, HostOperation::ClipboardWrite { .. }) => true,
        (Capability::WallpaperSet { sources }, HostOperation::SetWallpaper { source, .. }) => {
            sources.contains(source)
        }
        (Capability::ActionsInvoke { actions }, HostOperation::InvokeAction { action_id, .. }) => {
            actions
                .iter()
                .any(|pattern| wildcard_matches(pattern, action_id))
        }
        (Capability::NetworkHttp { .. }, HostOperation::HttpRequest { url, method, .. }) => {
            CanonicalHttpTarget::parse(url, method)
                .is_some_and(|target| capability_allows_http_target(capability, &target))
        }
        (Capability::LocationRead, HostOperation::LocationRead) => true,
        _ => false,
    }
}

pub fn capability_allows_http_target(
    capability: &Capability,
    target: &CanonicalHttpTarget,
) -> bool {
    match capability {
        Capability::NetworkHttp { hosts, paths } => {
            let host = target.host();
            let path = target.path();
            hosts.iter().any(|pattern| wildcard_matches(pattern, host))
                && (paths.is_empty() || paths.iter().any(|pattern| wildcard_matches(pattern, path)))
        }
        _ => false,
    }
}
