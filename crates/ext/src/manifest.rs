use crate::effects::{HostEffect, WallpaperSource};
use crate::events::EventKind;
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path};
use std::str::FromStr;

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;
pub const SUPPORTED_API_VERSION: &str = "0.2.0";

#[derive(Debug, PartialEq, Eq)]
pub enum ManifestError {
    InvalidExtensionId(String),
    InvalidContributionId(String),
    InvalidCanonicalId(String),
    ParseError(String),
    Validation(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidExtensionId(value) => write!(
                f,
                "invalid extension ID '{value}': expected lowercase reverse-domain segments"
            ),
            Self::InvalidContributionId(value) => write!(
                f,
                "invalid contribution ID '{value}': expected lowercase letters, digits, dashes, or underscores"
            ),
            Self::InvalidCanonicalId(value) => {
                write!(f, "invalid canonical contribution ID '{value}'")
            }
            Self::ParseError(message) => write!(f, "failed to parse TOML manifest: {message}"),
            Self::Validation(message) => write!(f, "invalid extension manifest: {message}"),
        }
    }
}

impl std::error::Error for ManifestError {}

#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct ExtensionId(String);

impl ExtensionId {
    pub fn new(id: impl Into<String>) -> Result<Self, ManifestError> {
        let value = id.into();
        let segments = value.split('.').collect::<Vec<_>>();
        let valid = segments.len() >= 3
            && segments.iter().all(|segment| {
                !segment.is_empty()
                    && segment
                        .bytes()
                        .next()
                        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
            });
        if !valid {
            return Err(ManifestError::InvalidExtensionId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ExtensionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl fmt::Display for ExtensionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct ContributionId(String);

impl ContributionId {
    pub fn new(id: impl Into<String>) -> Result<Self, ManifestError> {
        let value = id.into();
        let valid = value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
            });
        if !valid {
            return Err(ManifestError::InvalidContributionId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ContributionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl fmt::Display for ContributionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[schemars(with = "String")]
pub struct CanonicalId {
    pub extension_id: ExtensionId,
    pub contribution_id: ContributionId,
}

impl CanonicalId {
    pub fn new(extension_id: ExtensionId, contribution_id: ContributionId) -> Self {
        Self {
            extension_id,
            contribution_id,
        }
    }
}

impl FromStr for CanonicalId {
    type Err = ManifestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((extension_id, contribution_id)) = value.split_once('/') else {
            return Err(ManifestError::InvalidCanonicalId(value.to_owned()));
        };
        if contribution_id.contains('/') {
            return Err(ManifestError::InvalidCanonicalId(value.to_owned()));
        }
        Ok(Self::new(
            ExtensionId::new(extension_id)?,
            ContributionId::new(contribution_id)?,
        ))
    }
}

impl Serialize for CanonicalId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for CanonicalId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

impl fmt::Display for CanonicalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.extension_id, self.contribution_id)
    }
}

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
    pub desktop_widgets: Vec<DesktopWidgetContribution>,
    #[serde(default)]
    pub settings_pages: Vec<SettingsPageContribution>,
    #[serde(default)]
    pub side_panels: Vec<SidePanelContribution>,
    #[serde(default)]
    pub control_center: Vec<ControlCenterContribution>,
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
named_contribution!(DesktopWidgetContribution {
    description: Option<String>,
    default_width: Option<u32>,
    default_height: Option<u32>,
    min_width: Option<u32>,
    min_height: Option<u32>
});
named_contribution!(SettingsPageContribution { schema: String });
named_contribution!(SidePanelContribution {});
named_contribution!(ControlCenterContribution {});
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
    #[serde(rename = "process:exec")]
    ProcessExec,
    #[serde(rename = "filesystem:read")]
    FilesystemRead,
    #[serde(rename = "filesystem:write")]
    FilesystemWrite,
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
    #[serde(rename = "process:exec")]
    ProcessExec {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    #[serde(rename = "filesystem:read")]
    FilesystemRead { paths: Vec<String> },
    #[serde(rename = "filesystem:write")]
    FilesystemWrite { paths: Vec<String> },
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
            Self::ProcessExec { .. } => CapabilityKind::ProcessExec,
            Self::FilesystemRead { .. } => CapabilityKind::FilesystemRead,
            Self::FilesystemWrite { .. } => CapabilityKind::FilesystemWrite,
        }
    }

    pub fn allows_event(&self, event: EventKind) -> bool {
        matches!(self, Self::EventsSubscribe { events } if events.contains(&event))
    }

    pub fn allows_effect(&self, effect: &HostEffect) -> bool {
        match (self, effect) {
            (Self::NotificationsShow, HostEffect::ShowNotification { .. }) => true,
            (Self::WallpaperRead, HostEffect::WallpaperMetadataRead) => true,
            (Self::ThemeRead, HostEffect::ThemeRead) => true,
            (Self::ThemeSetSource, HostEffect::SetThemeSource { .. }) => true,
            (Self::ClipboardRead, HostEffect::ClipboardRead) => true,
            (Self::ClipboardWrite, HostEffect::ClipboardWrite { .. }) => true,
            (Self::WallpaperSet { sources }, HostEffect::SetWallpaper { source, .. }) => {
                sources.contains(source)
            }
            (Self::ActionsInvoke { actions }, HostEffect::InvokeAction { action_id, .. }) => {
                actions
                    .iter()
                    .any(|pattern| wildcard_matches(pattern, action_id))
            }
            (Self::NetworkHttp { hosts, paths }, HostEffect::HttpRequest { url, .. }) => {
                parse_http_target(url).is_some_and(|(host, path)| {
                    hosts.iter().any(|pattern| wildcard_matches(pattern, host))
                        && (paths.is_empty()
                            || paths.iter().any(|pattern| wildcard_matches(pattern, path)))
                })
            }
            (
                Self::ProcessExec {
                    command,
                    args: patterns,
                },
                HostEffect::ExecProcess {
                    command: actual,
                    args,
                },
            ) => wildcard_matches(command, actual) && arguments_match(patterns, args),
            (Self::FilesystemRead { paths }, HostEffect::ReadFile { path }) => {
                paths.iter().any(|pattern| wildcard_matches(pattern, path))
            }
            (Self::FilesystemWrite { paths }, HostEffect::WriteFile { path, .. }) => {
                paths.iter().any(|pattern| wildcard_matches(pattern, path))
            }
            _ => false,
        }
    }
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
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

fn arguments_match(patterns: &[String], arguments: &[String]) -> bool {
    if patterns.last().is_some_and(|pattern| pattern == "**") {
        arguments.len() >= patterns.len().saturating_sub(1)
            && patterns[..patterns.len() - 1]
                .iter()
                .zip(arguments)
                .all(|(pattern, argument)| wildcard_matches(pattern, argument))
    } else {
        patterns.len() == arguments.len()
            && patterns
                .iter()
                .zip(arguments)
                .all(|(pattern, argument)| wildcard_matches(pattern, argument))
    }
}

fn parse_http_target(url: &str) -> Option<(&str, &str)> {
    let remainder = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let path_start = remainder.find('/').unwrap_or(remainder.len());
    let authority = &remainder[..path_start];
    let path = if path_start == remainder.len() {
        "/"
    } else {
        &remainder[path_start..]
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host.split(':').next()?;
    (!host.is_empty()).then_some((host, path))
}

impl Contributions {
    fn entries(&self) -> impl Iterator<Item = (&ContributionId, &str)> {
        self.bar_widgets
            .iter()
            .map(|entry| (&entry.id, entry.name.as_str()))
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
                self.control_center
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
            | Capability::ClipboardWrite => true,
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
            Capability::ProcessExec { command, args } => {
                !command.trim().is_empty()
                    && !command.contains('*')
                    && args.iter().all(|arg| !arg.is_empty())
            }
            Capability::FilesystemRead { paths } | Capability::FilesystemWrite { paths } => {
                !paths.is_empty() && paths.iter().all(|path| valid_virtual_path_pattern(path))
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

fn valid_virtual_path_pattern(value: &str) -> bool {
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
