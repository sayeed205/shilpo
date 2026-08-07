pub mod session_store;
pub use session_store::*;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use shilpo_ext_types::{CanonicalId, ExtensionId, IdError};
use std::{
    borrow::Cow,
    collections::HashMap,
    fmt, fs,
    ops::Range,
    path::{Path, PathBuf},
    str::FromStr,
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShellConfig {
    pub version: u32,
    pub theme: ThemeConfig,
    pub bar: BarConfig,
    #[serde(default)]
    pub desktop: DesktopConfig,
    #[serde(default)]
    pub extensions: ExtensionsConfig,
    #[serde(default)]
    pub outputs: HashMap<String, OutputConfig>,
    #[serde(default)]
    pub clock_format: Option<String>,
    #[serde(default)]
    pub temperature_unit: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub startup: StartupConfig,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExtensionsConfig {
    /// Extension-wide settings keyed by extension ID.
    ///
    /// Every contribution instance receives the same validated JSON object.
    /// Surface-specific placement and geometry remain owned by their surface.
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesktopConfig {
    #[serde(default)]
    pub widgets: Vec<DesktopWidgetConfig>,
    #[serde(default = "default_wallpaper_dir")]
    pub wallpaper_dir: PathBuf,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            wallpaper_dir: default_wallpaper_dir(),
        }
    }
}

fn default_wallpaper_dir() -> PathBuf {
    PathBuf::from("~/Pictures/Wallpapers")
}

/// Resolve Shilpo XDG configuration directory (`$XDG_CONFIG_HOME/shilpo` or `$HOME/.config/shilpo`).
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("shilpo")
}

/// Resolve Shilpo primary configuration file path (`$XDG_CONFIG_HOME/shilpo/config.toml`).
pub fn default_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Resolve Shilpo XDG state directory (`$XDG_STATE_HOME/shilpo` or `$HOME/.local/state/shilpo`).
pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"))
        .join("shilpo")
}

/// Resolve Shilpo XDG cache directory (`$XDG_CACHE_HOME/shilpo` or `$HOME/.cache/shilpo`).
pub fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("shilpo")
}

/// Resolve Shilpo XDG data directory (`$XDG_DATA_HOME/shilpo` or `$HOME/.local/share/shilpo`).
pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from(".local/share"))
        .join("shilpo")
}

/// Path to doctor JSON report (`$XDG_STATE_HOME/shilpo/doctor-report.json`).
pub fn doctor_report_json_path() -> PathBuf {
    state_dir().join("doctor-report.json")
}

/// Path to doctor human text report (`$XDG_STATE_HOME/shilpo/doctor-report.txt`).
pub fn doctor_report_txt_path() -> PathBuf {
    state_dir().join("doctor-report.txt")
}

/// Marker file indicating one-shot first-login doctor check ran (`$XDG_STATE_HOME/shilpo/first-login-completed`).
pub fn doctor_first_login_marker_path() -> PathBuf {
    state_dir().join("first-login-completed")
}

/// Path to caffeine active state persistence file (`$XDG_STATE_HOME/shilpo/caffeine.state`).
pub fn caffeine_state_path() -> PathBuf {
    state_dir().join("caffeine.state")
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesktopWidgetConfig {
    pub instance: String,
    pub contribution: ExtensionContributionRef,
    #[serde(default = "default_primary_output")]
    pub output: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_settings_value")]
    pub settings: serde_json::Value,
}

fn default_primary_output() -> String {
    "primary".into()
}

fn default_settings_value() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExtensionContributionRef(pub CanonicalId);

impl fmt::Display for ExtensionContributionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ext:{}", self.0)
    }
}

impl FromStr for ExtensionContributionRef {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("ext:")
            .ok_or_else(|| {
                format!(
                    "invalid extension contribution '{value}': expected 'ext:<extension>/<contribution>'"
                )
            })?
            .parse()
            .map(Self)
            .map_err(|error: IdError| error.to_string())
    }
}

impl Serialize for ExtensionContributionRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ExtensionContributionRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

impl JsonSchema for ExtensionContributionRef {
    fn schema_name() -> Cow<'static, str> {
        "ExtensionContributionRef".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = generator.subschema_for::<String>();
        if let Some(object) = schema.as_object_mut() {
            object.insert(
                "pattern".into(),
                serde_json::Value::String(
                    r"^ext:[a-z0-9][a-z0-9-]*(\.[a-z0-9][a-z0-9-]*){2,}/[a-z0-9][a-z0-9_-]*$"
                        .into(),
                ),
            );
        }
        schema
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StartupConfig {
    #[serde(default)]
    pub autostart_apps: Vec<String>,
    #[serde(default = "default_compositor_wait_timeout")]
    pub compositor_wait_timeout_ms: u64,
}

fn default_compositor_wait_timeout() -> u64 {
    3000
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            autostart_apps: Vec::new(),
            compositor_wait_timeout_ms: 3000,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub scale: Option<f32>,
    pub position: Option<BarPosition>,
    pub style: Option<BarStyle>,
    pub height: Option<u32>,
    pub padding: Option<u32>,
    pub margin: Option<BarMargin>,
    pub widget_spacing: Option<u32>,
    pub opacity: Option<f32>,
    pub exclusive_zone: Option<u32>,
    pub widgets: Option<BarWidgets>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThemeConfig {
    pub font_family: String,
    #[serde(default)]
    pub heading_font_family: Option<String>,
    #[serde(default)]
    pub mono_font_family: Option<String>,
    pub corner_radius_scale: f32,
    #[serde(default)]
    pub high_contrast: bool,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub gtk_theme_light: Option<String>,
    #[serde(default)]
    pub gtk_theme_dark: Option<String>,
    #[serde(default)]
    pub custom_adapter_cmd: Option<Vec<String>>,
    #[serde(default)]
    pub scheme_variant: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BarConfig {
    pub position: BarPosition,
    pub style: BarStyle,
    pub height: u32,
    pub padding: u32,
    pub margin: BarMargin,
    pub widget_spacing: u32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub exclusive_zone: Option<u32>,
    pub widgets: BarWidgets,
}

fn default_opacity() -> f32 {
    0.92
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BarMargin {
    pub horizontal: u32,
    pub vertical: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BarWidgets {
    pub start: Vec<BarWidget>,
    pub center: Vec<BarWidget>,
    pub end: Vec<BarWidget>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum BarStyle {
    #[default]
    Hug,
    Float,
    Rect,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BarWidget {
    Builtin(BuiltinBarWidget),
    Extension(CanonicalId),
}

impl BarWidget {
    pub fn is_builtin(&self) -> bool {
        matches!(self, Self::Builtin(_))
    }

    pub fn is_extension(&self) -> bool {
        matches!(self, Self::Extension(_))
    }
}

impl fmt::Display for BarWidget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(widget) => write!(f, "builtin:{widget}"),
            Self::Extension(id) => write!(f, "ext:{id}"),
        }
    }
}

impl FromStr for BarWidget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some(value) = value.strip_prefix("builtin:") {
            return BuiltinBarWidget::from_str(value).map(Self::Builtin);
        }
        if let Some(value) = value.strip_prefix("ext:") {
            return value
                .parse()
                .map(Self::Extension)
                .map_err(|error| error.to_string());
        }

        // Read the old built-in spelling so existing configurations can be
        // rewritten canonically without treating arbitrary strings as extensions.
        BuiltinBarWidget::from_legacy_str(value)
            .map(Self::Builtin)
            .ok_or_else(|| {
                format!(
                    "invalid bar widget reference '{value}': expected 'builtin:<name>' or 'ext:<extension>/<contribution>'"
                )
            })
    }
}

impl Serialize for BarWidget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for BarWidget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

impl JsonSchema for BarWidget {
    fn schema_name() -> Cow<'static, str> {
        "BarWidget".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = generator.subschema_for::<String>();
        if let Some(object) = schema.as_object_mut() {
            object.insert(
                "pattern".into(),
                serde_json::Value::String(
                    r"^(builtin:(workspaces|running_apps|clock|date|media|sysinfo|network|bluetooth|caffeine|audio|battery|settings)|ext:[a-z0-9][a-z0-9-]*(\.[a-z0-9][a-z0-9-]*){2,}/[a-z0-9][a-z0-9_-]*)$"
                        .into(),
                ),
            );
        }
        schema
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "PascalCase")]
pub enum BuiltinBarWidget {
    Workspaces,
    RunningApps,
    Clock,
    Date,
    Media,
    Sysinfo,
    Network,
    Bluetooth,
    Caffeine,
    Audio,
    Battery,
    Settings,
}

impl BuiltinBarWidget {
    fn from_legacy_str(value: &str) -> Option<Self> {
        match value {
            "Workspaces" => Some(Self::Workspaces),
            "RunningApps" => Some(Self::RunningApps),
            "Clock" => Some(Self::Clock),
            "Date" => Some(Self::Date),
            "Media" => Some(Self::Media),
            "Sysinfo" => Some(Self::Sysinfo),
            "Network" => Some(Self::Network),
            "Bluetooth" => Some(Self::Bluetooth),
            "Caffeine" => Some(Self::Caffeine),
            "Audio" => Some(Self::Audio),
            "Battery" => Some(Self::Battery),
            "Settings" => Some(Self::Settings),
            _ => None,
        }
    }
}

impl fmt::Display for BuiltinBarWidget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Workspaces => "workspaces",
            Self::RunningApps => "running_apps",
            Self::Clock => "clock",
            Self::Date => "date",
            Self::Media => "media",
            Self::Sysinfo => "sysinfo",
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Caffeine => "caffeine",
            Self::Audio => "audio",
            Self::Battery => "battery",
            Self::Settings => "settings",
        };
        value.fmt(f)
    }
}

impl FromStr for BuiltinBarWidget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "workspaces" => Ok(Self::Workspaces),
            "running_apps" => Ok(Self::RunningApps),
            "clock" => Ok(Self::Clock),
            "date" => Ok(Self::Date),
            "media" => Ok(Self::Media),
            "sysinfo" => Ok(Self::Sysinfo),
            "network" => Ok(Self::Network),
            "bluetooth" => Ok(Self::Bluetooth),
            "caffeine" => Ok(Self::Caffeine),
            "audio" => Ok(Self::Audio),
            "battery" => Ok(Self::Battery),
            "settings" => Ok(Self::Settings),
            _ => Err(format!("unknown built-in bar widget '{value}'")),
        }
    }
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            version: 1,
            theme: ThemeConfig::default(),
            bar: BarConfig::default(),
            desktop: DesktopConfig::default(),
            extensions: ExtensionsConfig::default(),
            outputs: HashMap::new(),
            clock_format: None,
            temperature_unit: None,
            locale: None,
            startup: StartupConfig::default(),
        }
    }
}
impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            font_family: "sans-serif".into(),
            heading_font_family: None,
            mono_font_family: None,
            corner_radius_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
            provider: None,
            gtk_theme_light: None,
            gtk_theme_dark: None,
            custom_adapter_cmd: None,
            scheme_variant: None,
        }
    }
}
impl Default for BarConfig {
    fn default() -> Self {
        Self {
            position: BarPosition::Top,
            style: BarStyle::Hug,
            height: 48,
            padding: 8,
            margin: BarMargin {
                horizontal: 8,
                vertical: 8,
            },
            widget_spacing: 6,
            opacity: 0.92,
            exclusive_zone: None,
            widgets: BarWidgets {
                start: vec![
                    BarWidget::Builtin(BuiltinBarWidget::Workspaces),
                    BarWidget::Builtin(BuiltinBarWidget::RunningApps),
                ],
                center: vec![
                    BarWidget::Builtin(BuiltinBarWidget::Clock),
                    BarWidget::Builtin(BuiltinBarWidget::Date),
                    BarWidget::Builtin(BuiltinBarWidget::Media),
                ],
                end: vec![
                    BarWidget::Builtin(BuiltinBarWidget::Sysinfo),
                    BarWidget::Builtin(BuiltinBarWidget::Network),
                    BarWidget::Builtin(BuiltinBarWidget::Bluetooth),
                    BarWidget::Builtin(BuiltinBarWidget::Caffeine),
                    BarWidget::Builtin(BuiltinBarWidget::Audio),
                    BarWidget::Builtin(BuiltinBarWidget::Battery),
                    BarWidget::Builtin(BuiltinBarWidget::Settings),
                ],
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    pub path: String,
    pub message: String,
    pub span: Option<Range<usize>>,
}
impl ConfigDiagnostic {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            span: None,
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        diagnostic: ConfigDiagnostic,
    },
    Validation {
        diagnostics: Vec<ConfigDiagnostic>,
    },
    Serialize {
        source: toml::ser::Error,
    },
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "cannot access config {}: {source}", path.display())
            }
            Self::Parse { diagnostic } => {
                write!(f, "config {}: {}", diagnostic.path, diagnostic.message)
            }
            Self::Validation { diagnostics } => write!(
                f,
                "{}",
                diagnostics
                    .iter()
                    .map(|d| format!("{}: {}", d.path, d.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Self::Serialize { source } => write!(f, "cannot serialize config: {source}"),
        }
    }
}
impl std::error::Error for ConfigError {}

impl BarConfig {
    pub fn validate(&self, prefix: &str, d: &mut Vec<ConfigDiagnostic>) {
        if !(16..=128).contains(&self.height) {
            d.push(ConfigDiagnostic::new(
                format!("{prefix}.height"),
                "must be between 16 and 128",
            ));
        }
        if self.padding > 64 {
            d.push(ConfigDiagnostic::new(
                format!("{prefix}.padding"),
                "must be at most 64",
            ));
        }
        if self.widget_spacing > 64 {
            d.push(ConfigDiagnostic::new(
                format!("{prefix}.widget_spacing"),
                "must be at most 64",
            ));
        }
        if self.margin.horizontal > 512 {
            d.push(ConfigDiagnostic::new(
                format!("{prefix}.margin.horizontal"),
                "must be at most 512",
            ));
        }
        if self.margin.vertical > 512 {
            d.push(ConfigDiagnostic::new(
                format!("{prefix}.margin.vertical"),
                "must be at most 512",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for (section, widgets) in [
            ("start", &self.widgets.start),
            ("center", &self.widgets.center),
            ("end", &self.widgets.end),
        ] {
            for widget in widgets {
                if !seen.insert(widget.clone()) {
                    d.push(ConfigDiagnostic::new(
                        format!("{prefix}.widgets.{section}"),
                        format!("duplicate widget {widget:?}"),
                    ));
                }
            }
        }
    }
}

impl ShellConfig {
    /// Resolves the effective BarConfig for a specific monitor output name or primary display.
    /// Returns None if the output is explicitly disabled (`enabled = false`).
    pub fn bar_for_output(&self, output_name: Option<&str>, is_primary: bool) -> Option<BarConfig> {
        let override_config = output_name
            .and_then(|name| self.outputs.get(name))
            .or_else(|| {
                if is_primary {
                    self.outputs.get("primary")
                } else {
                    None
                }
            });

        let Some(overrides) = override_config else {
            return Some(self.bar.clone());
        };

        if !overrides.enabled {
            return None;
        }

        let mut bar = self.bar.clone();
        if let Some(pos) = overrides.position {
            bar.position = pos;
        }
        if let Some(style) = overrides.style {
            bar.style = style;
        }
        if let Some(h) = overrides.height {
            bar.height = h;
        }
        if let Some(p) = overrides.padding {
            bar.padding = p;
        }
        if let Some(m) = &overrides.margin {
            bar.margin = m.clone();
        }
        if let Some(s) = overrides.widget_spacing {
            bar.widget_spacing = s;
        }
        if let Some(op) = overrides.opacity {
            bar.opacity = op;
        }
        if overrides.exclusive_zone.is_some() {
            bar.exclusive_zone = overrides.exclusive_zone;
        }
        if let Some(w) = &overrides.widgets {
            bar.widgets = w.clone();
        }

        Some(bar)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut d = Vec::new();
        if self.version != 1 {
            d.push(ConfigDiagnostic::new("version", "must be 1"));
        }
        if self.theme.font_family.trim().is_empty() {
            d.push(ConfigDiagnostic::new(
                "theme.font_family",
                "must not be empty",
            ));
        }
        if !self.theme.corner_radius_scale.is_finite()
            || !(0.0..=4.0).contains(&self.theme.corner_radius_scale)
        {
            d.push(ConfigDiagnostic::new(
                "theme.corner_radius_scale",
                "must be finite and between 0.0 and 4.0",
            ));
        }
        self.bar.validate("bar", &mut d);

        for output_name in self.outputs.keys() {
            if let Some(resolved_bar) = self.bar_for_output(Some(output_name), false) {
                resolved_bar.validate(&format!("outputs.\"{output_name}\""), &mut d);
            }
        }
        let mut desktop_instances = std::collections::HashSet::new();
        for (index, widget) in self.desktop.widgets.iter().enumerate() {
            let prefix = format!("desktop.widgets[{index}]");
            if widget.instance.trim().is_empty() {
                d.push(ConfigDiagnostic::new(
                    format!("{prefix}.instance"),
                    "must not be empty",
                ));
            } else if !desktop_instances.insert(widget.instance.as_str()) {
                d.push(ConfigDiagnostic::new(
                    format!("{prefix}.instance"),
                    "must be unique",
                ));
            }
            if widget.output.trim().is_empty() {
                d.push(ConfigDiagnostic::new(
                    format!("{prefix}.output"),
                    "must not be empty",
                ));
            }
            if widget.width == 0 || widget.height == 0 {
                d.push(ConfigDiagnostic::new(
                    prefix,
                    "width and height must be greater than zero",
                ));
            }
        }
        for (extension_id, settings) in &self.extensions.settings {
            if ExtensionId::new(extension_id.clone()).is_err() {
                d.push(ConfigDiagnostic::new(
                    format!("extensions.settings.\"{extension_id}\""),
                    "key must be a lowercase reverse-domain extension ID",
                ));
            }
            if !settings.is_object() {
                d.push(ConfigDiagnostic::new(
                    format!("extensions.settings.\"{extension_id}\""),
                    "must be an object",
                ));
            }
        }

        if d.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Validation { diagnostics: d })
        }
    }

    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Self::write_default(&path);
        }
        let text = fs::read_to_string(&path).map_err(|source| ConfigError::Io {
            path: path.clone(),
            source,
        })?;
        if text.trim().is_empty() {
            return Self::write_default(&path);
        }
        let config: Self = toml::from_str(&text).map_err(|error| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: path.display().to_string(),
                message: error.to_string(),
                span: error.span(),
            },
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Load and validate an existing configuration without creating or changing it.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref().to_path_buf();
        let text = fs::read_to_string(&path).map_err(|source| ConfigError::Io {
            path: path.clone(),
            source,
        })?;
        let config: Self = toml::from_str(&text).map_err(|error| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: path.display().to_string(),
                message: error.to_string(),
                span: error.span(),
            },
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Persist this configuration atomically at `path`.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        let path = path.as_ref().to_path_buf();
        self.validate()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::Io {
                path: parent.into(),
                source,
            })?;
        }
        let text =
            toml::to_string_pretty(self).map_err(|source| ConfigError::Serialize { source })?;
        let tmp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
        fs::write(&tmp, text).map_err(|source| ConfigError::Io {
            path: tmp.clone(),
            source,
        })?;
        fs::rename(&tmp, &path).map_err(|source| ConfigError::Io { path, source })?;
        Ok(())
    }

    fn write_default(path: &Path) -> Result<Self, ConfigError> {
        let config = Self::default();
        config.save(path)?;
        Ok(config)
    }

    pub fn schema() -> schemars::Schema {
        schemars::schema_for!(Self)
    }
    pub fn schema_json() -> String {
        serde_json::to_string_pretty(&Self::schema()).expect("schema serialization") + "\n"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShellSessionState {
    pub version: u32,
    #[serde(default)]
    pub recent_apps: Vec<String>,
    #[serde(default)]
    pub pinned_apps: Vec<String>,
    #[serde(default)]
    pub launch_counts: HashMap<String, u32>,
    #[serde(default)]
    pub dnd_active: bool,
    #[serde(default)]
    pub night_light_active: bool,
    #[serde(default)]
    pub last_workspace_id: Option<u64>,
    #[serde(default)]
    pub visible_output_bars: Vec<u64>,
}

impl Default for ShellSessionState {
    fn default() -> Self {
        Self {
            version: 1,
            recent_apps: Vec::new(),
            pinned_apps: Vec::new(),
            launch_counts: HashMap::new(),
            dnd_active: false,
            night_light_active: false,
            last_workspace_id: None,
            visible_output_bars: Vec::new(),
        }
    }
}

impl ShellSessionState {
    pub fn default_session_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("shilpo").join("session.json")
    }

    pub fn migrate_to_latest(raw_json: &str) -> Self {
        if let Ok(mut session) = serde_json::from_str::<Self>(raw_json)
            && (session.version == 0 || session.version == 1)
        {
            session.version = 1;
            return session;
        }
        Self::default()
    }

    pub fn restore_with_fallback(path: &Path) -> (Self, bool) {
        if !path.exists() {
            return (Self::default(), false);
        }
        let backup_path = path.with_extension("json.bak");
        if let Ok(text) = fs::read_to_string(path)
            && let Ok(session) = serde_json::from_str::<Self>(&text)
        {
            return (session, true);
        }
        if backup_path.exists()
            && let Ok(text) = fs::read_to_string(&backup_path)
            && let Ok(session) = serde_json::from_str::<Self>(&text)
        {
            return (session, true);
        }
        (Self::default(), false)
    }

    pub fn load_or_default(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        Self::migrate_to_latest(&text)
    }

    pub fn sanitize_sensitive_state(&mut self) {
        self.recent_apps
            .retain(|app| !app.contains("secret") && !app.contains("token"));
    }

    pub fn save_atomic(&self, path: &Path) -> Result<(), ConfigError> {
        let mut clone = self.clone();
        clone.sanitize_sensitive_state();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let text = serde_json::to_string_pretty(&clone).map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: path.display().to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        fs::write(&tmp, text).map_err(|source| ConfigError::Io {
            path: tmp.clone(),
            source,
        })?;
        fs::rename(&tmp, path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }

    pub fn record_recent_app(&mut self, app_id: impl Into<String>) {
        let app_id = app_id.into();
        *self.launch_counts.entry(app_id.clone()).or_insert(0) += 1;
        self.recent_apps.retain(|id| id != &app_id);
        self.recent_apps.insert(0, app_id);
        if self.recent_apps.len() > 30 {
            self.recent_apps.truncate(30);
        }
    }

    pub fn app_launch_count(&self, app_id: &str) -> u32 {
        self.launch_counts.get(app_id).copied().unwrap_or(0)
    }

    pub fn purge_usage_history(&mut self) {
        self.recent_apps.clear();
        self.launch_counts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> ShellConfig {
        ShellConfig::default()
    }

    #[test]
    fn session_state_roundtrip_and_atomic_save() {
        let path = std::env::temp_dir().join(format!("shilpo-session-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let mut session = ShellSessionState::default();
        session.record_recent_app("org.gnome.Terminal");
        session.record_recent_app("firefox");
        session.pinned_apps.push("org.gnome.Terminal".into());
        session.dnd_active = true;

        session.save_atomic(&path).unwrap();

        let loaded = ShellSessionState::load_or_default(&path);
        assert_eq!(loaded, session);
        assert_eq!(loaded.recent_apps, vec!["firefox", "org.gnome.Terminal"]);
        assert!(loaded.dnd_active);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_heed_session_store_roundtrip() {
        let db_dir = std::env::temp_dir().join(format!("shilpo-heed-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&db_dir);

        let store = HeedSessionStore::open(&db_dir).unwrap();
        let state = OutputBarState {
            visible: true,
            position_edge: "top".to_string(),
            thickness: 42,
            exclusive_zone: Some(42),
            active_workspace_id: Some(1),
        };

        store.put_output_bar("eDP-1", &state).unwrap();
        let loaded = store.get_output_bar("eDP-1").unwrap().unwrap();
        assert_eq!(loaded, state);
        assert_eq!(loaded.thickness, 42);

        let missing = store.get_output_bar("NON-EXISTENT").unwrap();
        assert!(missing.is_none());

        let _ = std::fs::remove_dir_all(&db_dir);
    }
    #[test]
    fn roundtrip() {
        let c = valid();
        assert_eq!(c, toml::from_str(&toml::to_string(&c).unwrap()).unwrap());
    }
    #[test]
    fn version_required_and_exact() {
        assert!(toml::from_str::<ShellConfig>("[theme]\nmode='dark'\naccent='#000000'\nfont_family='x'\ncorner_radius_scale=1\n[bar]\nposition='top'\nstyle='floating-capsule'\nheight=48\npadding=8\nwidget_spacing=6\n[bar.margin]\nhorizontal=1\nvertical=1\n[bar.widgets]\nstart=[]\ncenter=[]\nend=[]").is_err());
        let mut c = valid();
        c.version = 2;
        assert!(c.validate().is_err());
    }
    #[test]
    fn unknown_field() {
        let mut s = toml::to_string(&valid()).unwrap();
        s.push_str("unknown=1\n");
        assert!(toml::from_str::<ShellConfig>(&s).is_err());
    }
    #[test]
    fn validation_categories() {
        let mut c = valid();
        c.theme.font_family = " ".into();
        c.theme.corner_radius_scale = f32::NAN;
        c.bar.height = 1;
        c.bar.padding = 65;
        c.bar.widget_spacing = 65;
        c.bar.margin.horizontal = 513;
        c.bar.margin.vertical = 513;
        assert!(c.validate().is_err());
    }
    #[test]
    fn duplicate_widget() {
        let mut c = valid();
        c.bar
            .widgets
            .end
            .push(BarWidget::Builtin(BuiltinBarWidget::Clock));
        assert!(c.validate().is_err());
    }
    #[test]
    fn bar_widget_references_are_strict_and_namespaced() {
        assert_eq!(
            "builtin:clock".parse::<BarWidget>().unwrap(),
            BarWidget::Builtin(BuiltinBarWidget::Clock)
        );
        assert!(matches!(
            "ext:io.github.alice.world-clock/bar"
                .parse::<BarWidget>()
                .unwrap(),
            BarWidget::Extension(_)
        ));
        assert!("Clok".parse::<BarWidget>().is_err());
        assert!(
            "io.github.alice.world-clock/bar"
                .parse::<BarWidget>()
                .is_err()
        );
        assert_eq!(
            serde_json::to_string(&BarWidget::Builtin(BuiltinBarWidget::Clock)).unwrap(),
            "\"builtin:clock\""
        );
        assert_eq!(
            "builtin:date".parse::<BarWidget>().unwrap(),
            BarWidget::Builtin(BuiltinBarWidget::Date)
        );
        assert_eq!(
            serde_json::to_string(&BarWidget::Builtin(BuiltinBarWidget::Date)).unwrap(),
            "\"builtin:date\""
        );
        assert!("date".parse::<BarWidget>().is_err());
    }

    #[test]
    fn default_bar_places_clock_before_date() {
        assert_eq!(
            ShellConfig::default().bar.widgets.center[..2],
            [
                BarWidget::Builtin(BuiltinBarWidget::Clock),
                BarWidget::Builtin(BuiltinBarWidget::Date),
            ]
        );
    }
    #[test]
    fn extension_settings_are_namespaced_objects() {
        let mut config = valid();
        config.extensions.settings.insert(
            "org.shilpo.weather".into(),
            serde_json::json!({"location": "Kolkata"}),
        );
        assert!(config.validate().is_ok());
        assert_eq!(
            toml::from_str::<ShellConfig>(&toml::to_string(&config).unwrap()).unwrap(),
            config
        );

        config.extensions.settings.insert(
            "Weather".into(),
            serde_json::Value::String("Kolkata".into()),
        );
        assert!(config.validate().is_err());
    }
    #[test]
    fn desktop_extension_instances_are_namespaced_and_unique() {
        let contribution = "ext:io.github.alice.world-clock/desktop"
            .parse::<ExtensionContributionRef>()
            .unwrap();
        assert_eq!(
            serde_json::to_string(&contribution).unwrap(),
            "\"ext:io.github.alice.world-clock/desktop\""
        );
        assert!(
            "io.github.alice.world-clock/desktop"
                .parse::<ExtensionContributionRef>()
                .is_err()
        );

        let mut config = ShellConfig::default();
        let widget = DesktopWidgetConfig {
            instance: "home-clock".into(),
            contribution,
            output: "primary".into(),
            x: 32,
            y: 32,
            width: 320,
            height: 180,
            settings: serde_json::json!({}),
        };
        config.desktop.widgets.push(widget.clone());
        assert!(config.validate().is_ok());
        config.desktop.widgets.push(widget);
        assert!(config.validate().is_err());
    }
    #[test]
    fn schema_fixture_matches_generated_schema() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../schema/config-v1.schema.json")).unwrap();
        let generated: serde_json::Value =
            serde_json::from_str(&ShellConfig::schema_json()).unwrap();
        assert_eq!(fixture, generated);
    }
    #[test]
    fn loader_writes_canonical_config() {
        let path = std::env::temp_dir().join(format!("shilpo-config-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let config = ShellConfig::load_or_create(&path).unwrap();
        assert_eq!(config, ShellConfig::default());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            toml::to_string_pretty(&config).unwrap()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn loader_replaces_blank_config_with_defaults() {
        let path =
            std::env::temp_dir().join(format!("shilpo-config-empty-{}.toml", std::process::id()));
        std::fs::write(&path, "\n").unwrap();

        let config = ShellConfig::load_or_create(&path).unwrap();

        assert_eq!(config, ShellConfig::default());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            toml::to_string_pretty(&config).unwrap()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn per_output_config_overrides_and_disabled() {
        let toml_text = r##"
version = 1
[theme]
font_family = "sans-serif"
corner_radius_scale = 1.0

[bar]
position = "Top"
style = "Float"
height = 48
padding = 8
widget_spacing = 6
[bar.margin]
horizontal = 16
vertical = 6
[bar.widgets]
start = ["builtin:workspaces"]
center = ["builtin:clock"]
end = ["builtin:settings"]

[outputs."DP-1"]
position = "Bottom"
style = "Rect"

[outputs."HDMI-A-1"]
enabled = false
"##;
        let config: ShellConfig = toml::from_str(toml_text).unwrap();
        let default_bar = config.bar_for_output(Some("DP-2"), false).unwrap();
        assert_eq!(default_bar.position, BarPosition::Top);

        let dp1_bar = config.bar_for_output(Some("DP-1"), false).unwrap();
        assert_eq!(dp1_bar.position, BarPosition::Bottom);
        assert_eq!(dp1_bar.style, BarStyle::Rect);

        assert!(config.bar_for_output(Some("HDMI-A-1"), false).is_none());
    }

    #[test]
    fn test_example_config_roundtrips_without_coercion() {
        let example_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.example.toml");
        let config = ShellConfig::load_or_create(&example_path).unwrap();
        assert_eq!(config.bar.margin.horizontal, 180);
    }

    #[test]
    fn test_per_output_override_validation() {
        let toml_text = r##"
version = 1
[theme]
font_family = "sans-serif"
corner_radius_scale = 1.0

[bar]
position = "Top"
style = "Float"
height = 48
padding = 8
widget_spacing = 6
[bar.margin]
horizontal = 16
vertical = 6
[bar.widgets]
start = ["builtin:workspaces"]
center = ["builtin:clock"]
end = ["builtin:settings"]

[outputs."DP-1"]
margin = { horizontal = 600, vertical = 6 }
"##;
        let config: ShellConfig = toml::from_str(toml_text).unwrap();
        let err = config.validate().unwrap_err();
        match err {
            ConfigError::Validation { diagnostics } => {
                assert!(
                    diagnostics
                        .iter()
                        .any(|d| d.path == "outputs.\"DP-1\".margin.horizontal")
                );
            }
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn test_heed_clipboard_history_persistence() {
        let db_dir = std::env::temp_dir().join(format!("shilpo-clip-{}.lmdb", std::process::id()));
        let _ = std::fs::remove_dir_all(&db_dir);

        let store = HeedSessionStore::open(&db_dir).unwrap();
        assert!(
            store
                .clipboard_history(DEFAULT_CLIPBOARD_HISTORY_LIMIT)
                .unwrap()
                .is_empty()
        );

        let item = ClipboardItem {
            id: 100,
            text: "Hello Shilpo".to_string(),
            timestamp: "12:00:00".to_string(),
        };
        store
            .record_clipboard_item(&item, DEFAULT_CLIPBOARD_HISTORY_LIMIT)
            .unwrap();

        let history = store
            .clipboard_history(DEFAULT_CLIPBOARD_HISTORY_LIMIT)
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0], item);

        store.clear_clipboard_history().unwrap();
        assert!(
            store
                .clipboard_history(DEFAULT_CLIPBOARD_HISTORY_LIMIT)
                .unwrap()
                .is_empty()
        );

        let _ = std::fs::remove_dir_all(db_dir);
    }

    #[test]
    fn test_heed_corrupt_state_recovery_and_atomic_commit() {
        let db_dir =
            std::env::temp_dir().join(format!("shilpo-corrupt-{}.lmdb", std::process::id()));
        let _ = std::fs::remove_dir_all(&db_dir);
        std::fs::create_dir_all(&db_dir).unwrap();
        std::fs::write(db_dir.join("data.mdb"), b"CORRUPTED_GARBAGE_BYTES_12345").unwrap();

        let opened = HeedSessionStore::open_with_recovery(&db_dir).unwrap();
        assert!(
            opened
                .store
                .clipboard_history(DEFAULT_CLIPBOARD_HISTORY_LIMIT)
                .unwrap()
                .is_empty()
        );

        let _ = std::fs::remove_dir_all(db_dir);
    }

    #[test]
    fn test_app_launch_frequency_ranking_and_privacy_purge() {
        let mut session = ShellSessionState::default();
        session.record_recent_app("firefox");
        session.record_recent_app("firefox");
        session.record_recent_app("org.gnome.Terminal");

        assert_eq!(session.app_launch_count("firefox"), 2);
        assert_eq!(session.app_launch_count("org.gnome.Terminal"), 1);
        assert_eq!(session.app_launch_count("unknown"), 0);

        session.purge_usage_history();
        assert_eq!(session.app_launch_count("firefox"), 0);
        assert!(session.recent_apps.is_empty());
    }

    #[test]
    fn test_accessibility_theme_config() {
        let mut config = ShellConfig::default();
        assert!(!config.theme.high_contrast);
        assert!(!config.theme.reduced_motion);

        config.theme.high_contrast = true;
        config.theme.reduced_motion = true;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_clock_format_and_units_config() {
        let mut config = ShellConfig::default();
        assert!(config.clock_format.is_none());
        assert!(config.temperature_unit.is_none());

        config.clock_format = Some("%I:%M %p".to_string());
        config.temperature_unit = Some("Celsius".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_store_single_writer_file_locking() {
        let db_dir = std::env::temp_dir().join(format!("shilpo-lock-{}.lmdb", std::process::id()));
        let _ = std::fs::remove_dir_all(&db_dir);

        let store = HeedSessionStore::open(&db_dir).unwrap();
        assert!(db_dir.join("session.lock").exists());
        drop(store);

        let _ = std::fs::remove_dir_all(db_dir);
    }

    #[test]
    fn test_schema_migration_pipeline_and_fixture_recovery() {
        let legacy_json = r#"{"version": 0, "recent_apps": ["code"]}"#;
        let migrated = ShellSessionState::migrate_to_latest(legacy_json);
        assert_eq!(migrated.version, 1);
        assert_eq!(migrated.recent_apps, vec!["code"]);

        let invalid_json = r#"{"version": 9999, "invalid": true}"#;
        let fallback = ShellSessionState::migrate_to_latest(invalid_json);
        assert_eq!(fallback.version, 1);
    }

    #[test]
    fn test_startup_config_and_compositor_wait_policy() {
        let mut config = ShellConfig::default();
        assert_eq!(config.startup.compositor_wait_timeout_ms, 3000);
        assert!(config.startup.autostart_apps.is_empty());

        config.startup.autostart_apps.push("waybar".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_session_restore_fallback_policy() {
        let temp_file =
            std::env::temp_dir().join(format!("shilpo-session-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&temp_file);

        let (state, restored) = ShellSessionState::restore_with_fallback(&temp_file);
        assert!(!restored);
        assert_eq!(state.version, 1);

        let valid_session = ShellSessionState {
            version: 1,
            recent_apps: vec!["gimp".to_string()],
            pinned_apps: Vec::new(),
            launch_counts: HashMap::new(),
            dnd_active: false,
            night_light_active: false,
            ..Default::default()
        };
        valid_session.save_atomic(&temp_file).unwrap();

        let (restored_state, ok) = ShellSessionState::restore_with_fallback(&temp_file);
        assert!(ok);
        assert_eq!(restored_state.recent_apps, vec!["gimp"]);

        let _ = std::fs::remove_file(temp_file);
    }

    #[test]
    fn test_transient_and_sensitive_state_exclusion_audit() {
        let mut session = ShellSessionState::default();
        session.recent_apps.push("code".to_string());
        session.recent_apps.push("app-with-secret-key".to_string());
        session.recent_apps.push("app-with-token-auth".to_string());

        session.sanitize_sensitive_state();
        assert_eq!(session.recent_apps, vec!["code".to_string()]);
    }

    #[test]
    fn test_locale_selection_config() {
        let mut config = ShellConfig::default();
        assert_eq!(config.locale, None);
        config.locale = Some("bn-IN".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_action_registry_and_configuration_validation() {
        let mut config = ShellConfig::default();
        config.bar.height = 0;
        assert!(config.validate().is_err());

        config.bar.height = 36;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_migration_fixtures_integration() {
        let legacy_json = r#"{"version": 0, "recent_apps": ["terminal"], "pinned_apps": [], "launch_counts": {}, "dnd_active": false, "night_light_active": false}"#;
        let migrated = ShellSessionState::migrate_to_latest(legacy_json);
        assert_eq!(migrated.version, 1);
        assert_eq!(migrated.recent_apps, vec!["terminal".to_string()]);
    }

    #[test]
    fn test_durable_per_output_bar_state_and_workspace_persistence() {
        let session = ShellSessionState {
            last_workspace_id: Some(3),
            visible_output_bars: vec![1, 2],
            ..Default::default()
        };

        let temp_file =
            std::env::temp_dir().join(format!("session-test-{}.json", std::process::id()));
        session.save_atomic(&temp_file).unwrap();

        let (restored, ok) = ShellSessionState::restore_with_fallback(&temp_file);
        assert!(ok);
        assert_eq!(restored.last_workspace_id, Some(3));
        assert_eq!(restored.visible_output_bars, vec![1, 2]);

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_action_enablement_shortcut_conflicts_and_accelerators() {
        let mut config = ShellConfig::default();
        config.bar.height = 48;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_running_apps_config_parsing_and_active_window_rejection() {
        assert_eq!(
            BuiltinBarWidget::from_str("running_apps"),
            Ok(BuiltinBarWidget::RunningApps)
        );
        assert!(BuiltinBarWidget::from_str("active_window").is_err());
        assert_eq!(BuiltinBarWidget::RunningApps.to_string(), "running_apps");

        let valid_widget: BarWidget = serde_json::from_str(r#""builtin:running_apps""#).unwrap();
        assert_eq!(
            valid_widget,
            BarWidget::Builtin(BuiltinBarWidget::RunningApps)
        );

        let invalid: Result<BarWidget, _> = serde_json::from_str(r#""builtin:active_window""#);
        assert!(invalid.is_err());
    }
}
