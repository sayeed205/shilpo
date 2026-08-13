use crate::effects::WallpaperSource;
use crate::events::EventKind;
use crate::id::{ContributionId, ExtensionId, IdError};
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path};

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;
pub const SUPPORTED_API_VERSION: &str = "0.1.0";

#[derive(Debug, PartialEq, Eq)]
pub enum ManifestError {
    Id(IdError),
    ParseError(String),
    Validation(String),
}

impl From<IdError> for ManifestError {
    fn from(value: IdError) -> Self {
        Self::Id(value)
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(err) => write!(f, "{err}"),
            Self::ParseError(message) => write!(f, "failed to parse TOML manifest: {message}"),
            Self::Validation(message) => write!(f, "invalid extension manifest: {message}"),
        }
    }
}

impl std::error::Error for ManifestError {}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    pub id: ExtensionId,
    pub name: String,
    #[schemars(with = "String")]
    pub version: Version,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_api_version")]
    #[schemars(with = "String")]
    pub api_version: Version,
    #[serde(default = "default_api_version")]
    #[schemars(with = "String")]
    pub min_shilpo_version: Version,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub library: Option<LibraryConfig>,
    #[serde(default)]
    pub contributions: Contributions,
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

fn default_schema_version() -> u32 {
    SUPPORTED_SCHEMA_VERSION
}

fn default_api_version() -> Version {
    Version::parse(SUPPORTED_API_VERSION).expect("the supported API version is valid")
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LibraryConfig {
    pub path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Contributions {
    #[serde(default)]
    pub bar_widgets: Vec<BarWidgetContribution>,
    #[serde(default)]
    pub bar_menus: Vec<BarMenuContribution>,
    #[serde(default)]
    pub desktop_widgets: Vec<DesktopWidgetContribution>,
    #[serde(default)]
    pub settings_pages: Vec<SettingsPageContribution>,
    #[serde(default)]
    pub side_panels: Vec<SidePanelContribution>,
    #[serde(default)]
    pub launcher_providers: Vec<LauncherProviderContribution>,
    #[serde(default)]
    pub actions: Vec<ActionContribution>,
    #[serde(default)]
    pub background_tasks: Vec<BackgroundTaskContribution>,
}

macro_rules! named_contribution {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            pub id: ContributionId,
            pub name: String,
            $(pub $field: $ty,)*
        }
    };
}

named_contribution!(BarWidgetContribution {
    description: Option<String>
});
named_contribution!(BarMenuContribution {
    bar_widget: ContributionId
});
named_contribution!(DesktopWidgetContribution {
    description: Option<String>,
    default_width: Option<u32>,
    default_height: Option<u32>,
    min_width: Option<u32>,
    min_height: Option<u32>
});
named_contribution!(SettingsPageContribution { schema: String });
named_contribution!(SidePanelContribution {});
named_contribution!(LauncherProviderContribution {});
named_contribution!(ActionContribution {});
named_contribution!(BackgroundTaskContribution {});

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct Subscription {
    pub event: EventKind,
}

#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
pub enum CapabilityKind {
    #[serde(rename = "events:subscribe")]
    EventsSubscribe,
    #[serde(rename = "wallpaper:set")]
    WallpaperSet,
    #[serde(rename = "wallpaper:read")]
    WallpaperRead,
    #[serde(rename = "theme:read")]
    ThemeRead,
    #[serde(rename = "theme:set_source")]
    ThemeSetSource,
    #[serde(rename = "notifications:show")]
    NotificationsShow,
    #[serde(rename = "clipboard:read")]
    ClipboardRead,
    #[serde(rename = "clipboard:write")]
    ClipboardWrite,
    #[serde(rename = "actions:invoke")]
    ActionsInvoke,
    #[serde(rename = "network:http")]
    NetworkHttp,
    #[serde(rename = "filesystem:read")]
    FilesystemRead,
    #[serde(rename = "filesystem:write")]
    FilesystemWrite,
    #[serde(rename = "location:read")]
    LocationRead,
    #[serde(rename = "secrets")]
    Secrets,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Capability {
    #[serde(rename = "events:subscribe")]
    EventsSubscribe { events: Vec<EventKind> },
    #[serde(rename = "wallpaper:set")]
    WallpaperSet { sources: Vec<WallpaperSource> },
    #[serde(rename = "wallpaper:read")]
    WallpaperRead,
    #[serde(rename = "theme:read")]
    ThemeRead,
    #[serde(rename = "theme:set_source")]
    ThemeSetSource,
    #[serde(rename = "notifications:show")]
    NotificationsShow,
    #[serde(rename = "clipboard:read")]
    ClipboardRead,
    #[serde(rename = "clipboard:write")]
    ClipboardWrite,
    #[serde(rename = "actions:invoke")]
    ActionsInvoke { actions: Vec<String> },
    #[serde(rename = "network:http")]
    NetworkHttp {
        hosts: Vec<String>,
        #[serde(default)]
        paths: Vec<String>,
    },
    #[serde(rename = "filesystem:read")]
    FilesystemRead { paths: Vec<String> },
    #[serde(rename = "filesystem:write")]
    FilesystemWrite { paths: Vec<String> },
    #[serde(rename = "location:read")]
    LocationRead,
    #[serde(rename = "secrets")]
    Secrets { purposes: Vec<SecretPurpose> },
}

impl Capability {
    pub fn kind(&self) -> CapabilityKind {
        match self {
            Self::EventsSubscribe { .. } => CapabilityKind::EventsSubscribe,
            Self::WallpaperSet { .. } => CapabilityKind::WallpaperSet,
            Self::WallpaperRead => CapabilityKind::WallpaperRead,
            Self::ThemeRead => CapabilityKind::ThemeRead,
            Self::ThemeSetSource => CapabilityKind::ThemeSetSource,
            Self::NotificationsShow => CapabilityKind::NotificationsShow,
            Self::ClipboardRead => CapabilityKind::ClipboardRead,
            Self::ClipboardWrite => CapabilityKind::ClipboardWrite,
            Self::ActionsInvoke { .. } => CapabilityKind::ActionsInvoke,
            Self::NetworkHttp { .. } => CapabilityKind::NetworkHttp,
            Self::FilesystemRead { .. } => CapabilityKind::FilesystemRead,
            Self::FilesystemWrite { .. } => CapabilityKind::FilesystemWrite,
            Self::LocationRead => CapabilityKind::LocationRead,
            Self::Secrets { .. } => CapabilityKind::Secrets,
        }
    }

    pub fn allows_event(&self, event: EventKind) -> bool {
        matches!(self, Self::EventsSubscribe { events } if events.contains(&event))
    }
}

pub fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let (mut pattern_index, mut value_index, mut star, mut retry) = (0, 0, None, 0);
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    while value_index < value.len() {
        if pattern_index < pattern.len() && pattern[pattern_index] == value[value_index] {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            pattern_index += 1;
            retry = value_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            retry += 1;
            value_index = retry;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

impl Contributions {
    fn entries(&self) -> impl Iterator<Item = (&ContributionId, &str)> {
        self.bar_widgets
            .iter()
            .map(|entry| (&entry.id, entry.name.as_str()))
            .chain(
                self.bar_menus
                    .iter()
                    .map(|entry| (&entry.id, entry.name.as_str())),
            )
            .chain(
                self.desktop_widgets
                    .iter()
                    .map(|entry| (&entry.id, entry.name.as_str())),
            )
            .chain(
                self.settings_pages
                    .iter()
                    .map(|entry| (&entry.id, entry.name.as_str())),
            )
            .chain(
                self.side_panels
                    .iter()
                    .map(|entry| (&entry.id, entry.name.as_str())),
            )
            .chain(
                self.launcher_providers
                    .iter()
                    .map(|entry| (&entry.id, entry.name.as_str())),
            )
            .chain(
                self.actions
                    .iter()
                    .map(|entry| (&entry.id, entry.name.as_str())),
            )
            .chain(
                self.background_tasks
                    .iter()
                    .map(|entry| (&entry.id, entry.name.as_str())),
            )
    }

    pub fn contains(&self, id: &ContributionId) -> bool {
        self.entries().any(|(candidate, _)| candidate == id)
    }
}

impl ExtensionManifest {
    pub fn from_toml(source: &str) -> Result<Self, ManifestError> {
        let manifest: Self =
            toml::from_str(source).map_err(|error| ManifestError::ParseError(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.name.trim().is_empty() {
            return Err(ManifestError::Validation(
                "the extension name cannot be empty".into(),
            ));
        }
        if self.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(ManifestError::Validation(format!(
                "schema version {} is unsupported; expected {SUPPORTED_SCHEMA_VERSION}",
                self.schema_version
            )));
        }
        if self.api_version != default_api_version() {
            return Err(ManifestError::Validation(format!(
                "API version {} is unsupported; expected {SUPPORTED_API_VERSION}",
                self.api_version
            )));
        }
        if let Some(library) = &self.library {
            validate_relative_path("library.path", &library.path)?;
        }

        let mut contribution_ids = HashSet::new();
        for (id, name) in self.contributions.entries() {
            if !contribution_ids.insert(id) {
                return Err(ManifestError::Validation(format!(
                    "duplicate contribution ID '{id}'"
                )));
            }
            if name.trim().is_empty() {
                return Err(ManifestError::Validation(format!(
                    "contribution '{id}' has an empty name"
                )));
            }
        }
        for page in &self.contributions.settings_pages {
            validate_relative_path("settings page schema", &page.schema)?;
        }
        let mut bar_widget_targets = HashSet::new();
        for menu in &self.contributions.bar_menus {
            let target_exists = self
                .contributions
                .bar_widgets
                .iter()
                .any(|bw| bw.id == menu.bar_widget);
            if !target_exists {
                return Err(ManifestError::Validation(format!(
                    "bar menu '{}' targets unknown bar widget '{}'",
                    menu.id, menu.bar_widget
                )));
            }
            if !bar_widget_targets.insert(&menu.bar_widget) {
                return Err(ManifestError::Validation(format!(
                    "multiple bar menus target bar widget '{}'",
                    menu.bar_widget
                )));
            }
        }
        for widget in &self.contributions.desktop_widgets {
            if let (Some(minimum), Some(default)) = (widget.min_width, widget.default_width)
                && default < minimum
            {
                return Err(ManifestError::Validation(format!(
                    "desktop widget '{}' default width is below its minimum",
                    widget.id
                )));
            }
            if let (Some(minimum), Some(default)) = (widget.min_height, widget.default_height)
                && default < minimum
            {
                return Err(ManifestError::Validation(format!(
                    "desktop widget '{}' default height is below its minimum",
                    widget.id
                )));
            }
        }

        let mut subscriptions = HashSet::new();
        for subscription in &self.subscriptions {
            if !subscriptions.insert(subscription.event) {
                return Err(ManifestError::Validation(format!(
                    "duplicate subscription for {:?}",
                    subscription.event
                )));
            }
            if !self
                .capabilities
                .iter()
                .any(|capability| capability.allows_event(subscription.event))
            {
                return Err(ManifestError::Validation(format!(
                    "subscription {:?} is not covered by events:subscribe",
                    subscription.event
                )));
            }
        }
        validate_capabilities(&self.capabilities)
    }

    pub fn schema_json() -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&schemars::schema_for!(Self))
    }
}

fn validate_relative_path(field: &str, value: &str) -> Result<(), ManifestError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ManifestError::Validation(format!(
            "{field} must be a safe relative path"
        )));
    }
    Ok(())
}

fn validate_capabilities(capabilities: &[Capability]) -> Result<(), ManifestError> {
    let mut kinds = HashSet::new();
    for capability in capabilities {
        if !kinds.insert(capability.kind()) {
            return Err(ManifestError::Validation(format!(
                "duplicate capability {:?}",
                capability.kind()
            )));
        }
        let valid = match capability {
            Capability::EventsSubscribe { events } => !events.is_empty(),
            Capability::WallpaperSet { sources } => !sources.is_empty(),
            Capability::WallpaperRead
            | Capability::ThemeRead
            | Capability::ThemeSetSource
            | Capability::NotificationsShow
            | Capability::ClipboardRead
            | Capability::ClipboardWrite
            | Capability::LocationRead => true,
            Capability::ActionsInvoke { actions } => {
                !actions.is_empty() && actions.iter().all(|action| !action.trim().is_empty())
            }
            Capability::NetworkHttp { hosts, paths } => {
                !hosts.is_empty()
                    && hosts.iter().all(|host| {
                        !host.trim().is_empty() && !host.contains('/') && !host.contains("://")
                    })
                    && paths.iter().all(|path| path.starts_with('/'))
            }
            Capability::FilesystemRead { paths } | Capability::FilesystemWrite { paths } => {
                !paths.is_empty() && paths.iter().all(|path| valid_virtual_path_pattern(path))
            }
            Capability::Secrets { purposes } => {
                if purposes.is_empty() {
                    false
                } else {
                    let mut seen = HashSet::new();
                    purposes.iter().all(|p| {
                        SecretPurpose::validate(p.as_str()).is_ok() && seen.insert(p.as_str())
                    })
                }
            }
        };
        if !valid {
            return Err(ManifestError::Validation(format!(
                "capability {:?} has an empty or invalid scope",
                capability.kind()
            )));
        }
    }
    Ok(())
}

pub fn valid_virtual_path_pattern(value: &str) -> bool {
    let path = Path::new(value);
    !value.trim().is_empty()
        && !path.is_absolute()
        && matches!(
            path.components().next(),
            Some(Component::Normal(root))
                if root == "assets" || root == "data" || root == "user"
        )
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct SecretPurpose(String);

impl SecretPurpose {
    pub fn new(value: impl Into<String>) -> Result<Self, ManifestError> {
        let value = value.into();
        Self::validate(&value)?;
        Ok(Self(value))
    }

    pub fn parse(value: &str) -> Result<Self, ManifestError> {
        Self::validate(value)?;
        Ok(Self(value.to_string()))
    }

    pub fn validate(value: &str) -> Result<(), ManifestError> {
        if value.is_empty() || value.len() > 64 {
            return Err(ManifestError::Validation(
                "secret purpose must be 1 to 64 bytes".into(),
            ));
        }
        let bytes = value.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return Err(ManifestError::Validation(
                "secret purpose must start with a lowercase ASCII letter".into(),
            ));
        }
        for &b in &bytes[1..] {
            if !b.is_ascii_lowercase() && !b.is_ascii_digit() && b != b'-' {
                return Err(ManifestError::Validation(format!(
                    "invalid character '{:?}' in secret purpose",
                    b as char
                )));
            }
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SecretPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for SecretPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretPurpose({})", self.0)
    }
}

impl std::str::FromStr for SecretPurpose {
    type Err = ManifestError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for SecretPurpose {
    type Error = ManifestError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl From<SecretPurpose> for String {
    fn from(p: SecretPurpose) -> Self {
        p.0
    }
}

impl AsRef<str> for SecretPurpose {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for SecretPurpose {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
pub struct SecretRef {
    #[serde(rename = "secret_ref")]
    pub handle: String,
}

impl SecretRef {
    pub fn new(handle: impl Into<String>) -> Self {
        Self {
            handle: handle.into(),
        }
    }
}

impl fmt::Debug for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretRef")
            .field("handle", &"<redacted>")
            .finish()
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretRef(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_bar_menu_manifest() {
        let toml = r#"
            id = "org.shilpo.weather"
            name = "Weather App"
            version = "1.0.0"

            [[contributions.bar_widgets]]
            id = "weather-widget"
            name = "Weather"

            [[contributions.bar_menus]]
            id = "weather-menu"
            name = "Weather Details"
            bar_widget = "weather-widget"
        "#;
        let manifest = ExtensionManifest::from_toml(toml).unwrap();
        assert_eq!(manifest.contributions.bar_menus.len(), 1);
        assert_eq!(
            manifest.contributions.bar_menus[0].id.as_str(),
            "weather-menu"
        );
        assert_eq!(
            manifest.contributions.bar_menus[0].bar_widget.as_str(),
            "weather-widget"
        );
    }

    #[test]
    fn test_bar_menu_missing_target_fails() {
        let toml = r#"
            id = "org.shilpo.weather"
            name = "Weather App"
            version = "1.0.0"

            [[contributions.bar_menus]]
            id = "weather-menu"
            name = "Weather Details"
            bar_widget = "nonexistent-widget"
        "#;
        let err = ExtensionManifest::from_toml(toml).unwrap_err();
        assert!(err.to_string().contains("unknown bar widget"));
    }

    #[test]
    fn test_bar_menu_duplicate_target_fails() {
        let toml = r#"
            id = "org.shilpo.weather"
            name = "Weather App"
            version = "1.0.0"

            [[contributions.bar_widgets]]
            id = "weather-widget"
            name = "Weather"

            [[contributions.bar_menus]]
            id = "weather-menu-1"
            name = "Weather Details 1"
            bar_widget = "weather-widget"

            [[contributions.bar_menus]]
            id = "weather-menu-2"
            name = "Weather Details 2"
            bar_widget = "weather-widget"
        "#;
        let err = ExtensionManifest::from_toml(toml).unwrap_err();
        assert!(
            err.to_string()
                .contains("multiple bar menus target bar widget")
        );
    }

    #[test]
    fn test_bar_menu_duplicate_contribution_id_fails() {
        let toml = r#"
            id = "org.shilpo.weather"
            name = "Weather App"
            version = "1.0.0"

            [[contributions.bar_widgets]]
            id = "weather-item"
            name = "Weather"

            [[contributions.bar_menus]]
            id = "weather-item"
            name = "Weather Details"
            bar_widget = "weather-item"
        "#;
        let err = ExtensionManifest::from_toml(toml).unwrap_err();
        assert!(err.to_string().contains("duplicate contribution ID"));
    }

    #[test]
    fn test_bar_menu_unknown_sizing_field_fails() {
        let toml = r#"
            id = "org.shilpo.weather"
            name = "Weather App"
            version = "1.0.0"

            [[contributions.bar_widgets]]
            id = "weather-widget"
            name = "Weather"

            [[contributions.bar_menus]]
            id = "weather-menu"
            name = "Weather Details"
            bar_widget = "weather-widget"
            width = 300
        "#;
        let err = ExtensionManifest::from_toml(toml).unwrap_err();
        assert!(matches!(err, ManifestError::ParseError(_)));
    }
}
