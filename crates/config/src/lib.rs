use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt, fs,
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShellConfig {
    pub version: u32,
    pub theme: ThemeConfig,
    pub bar: BarConfig,
    #[serde(default)]
    pub outputs: HashMap<String, OutputConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub position: Option<BarPosition>,
    pub style: Option<BarStyle>,
    pub height: Option<u32>,
    pub padding: Option<u32>,
    pub margin: Option<BarMargin>,
    pub widget_spacing: Option<u32>,
    pub exclusive_zone: Option<u32>,
    pub widgets: Option<BarWidgets>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThemeConfig {
    pub mode: ThemeMode,
    pub accent: String,
    pub font_family: String,
    pub corner_radius_scale: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    Auto,
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
    #[serde(default)]
    pub exclusive_zone: Option<u32>,
    pub widgets: BarWidgets,
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
#[serde(rename_all = "kebab-case")]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BarStyle {
    #[default]
    FloatingCapsule,
    FullEdge,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum BarWidget {
    Launcher,
    Workspaces,
    ActiveWindow,
    Clock,
    Media,
    Sysinfo,
    Network,
    Audio,
    Battery,
    Settings,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            version: 1,
            theme: ThemeConfig::default(),
            bar: BarConfig::default(),
            outputs: HashMap::new(),
        }
    }
}
impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            accent: "#6750A4".into(),
            font_family: "sans-serif".into(),
            corner_radius_scale: 1.0,
        }
    }
}
impl Default for BarConfig {
    fn default() -> Self {
        Self {
            position: BarPosition::Top,
            style: BarStyle::FloatingCapsule,
            height: 48,
            padding: 8,
            margin: BarMargin {
                horizontal: 16,
                vertical: 6,
            },
            widget_spacing: 6,
            exclusive_zone: None,
            widgets: BarWidgets {
                start: vec![
                    BarWidget::Launcher,
                    BarWidget::Workspaces,
                    BarWidget::ActiveWindow,
                ],
                center: vec![BarWidget::Clock, BarWidget::Media],
                end: vec![
                    BarWidget::Sysinfo,
                    BarWidget::Network,
                    BarWidget::Audio,
                    BarWidget::Battery,
                    BarWidget::Settings,
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
                if !seen.insert(*widget) {
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
        let accent = self.theme.accent.as_bytes();
        let valid_hex = (accent.len() == 7 || accent.len() == 9)
            && accent[0] == b'#'
            && accent[1..].iter().all(u8::is_ascii_hexdigit);
        if !valid_hex {
            d.push(ConfigDiagnostic::new(
                "theme.accent",
                "must be #RRGGBB or #AARRGGBB",
            ));
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

    fn write_default(path: &Path) -> Result<Self, ConfigError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::Io {
                path: parent.into(),
                source,
            })?;
        }
        let config = Self::default();
        let text =
            toml::to_string_pretty(&config).map_err(|source| ConfigError::Serialize { source })?;
        let tmp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
        fs::write(&tmp, text).map_err(|source| ConfigError::Io {
            path: tmp.clone(),
            source,
        })?;
        fs::rename(&tmp, path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(config)
    }

    pub fn schema() -> schemars::Schema {
        schemars::schema_for!(Self)
    }
    pub fn schema_json() -> String {
        serde_json::to_string_pretty(&Self::schema()).expect("schema serialization") + "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> ShellConfig {
        ShellConfig::default()
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
        c.theme.accent = "bad".into();
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
        c.bar.widgets.end.push(BarWidget::Clock);
        assert!(c.validate().is_err());
    }
    #[test]
    fn schema_fixture_matches_generated_schema() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../schema/config-v1.schema.json")).unwrap();
        let generated: serde_json::Value =
            serde_json::from_str(&ShellConfig::schema_json()).unwrap();
        assert_eq!(fixture, generated,);
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
mode = "dark"
accent = "#6750A4"
font_family = "sans-serif"
corner_radius_scale = 1.0

[bar]
position = "top"
style = "floating-capsule"
height = 48
padding = 8
widget_spacing = 6
[bar.margin]
horizontal = 16
vertical = 6
[bar.widgets]
start = ["launcher"]
center = ["clock"]
end = ["settings"]

[outputs."DP-1"]
position = "bottom"
style = "full-edge"

[outputs."HDMI-A-1"]
enabled = false
"##;
        let config: ShellConfig = toml::from_str(toml_text).unwrap();
        let default_bar = config.bar_for_output(Some("DP-2"), false).unwrap();
        assert_eq!(default_bar.position, BarPosition::Top);

        let dp1_bar = config.bar_for_output(Some("DP-1"), false).unwrap();
        assert_eq!(dp1_bar.position, BarPosition::Bottom);
        assert_eq!(dp1_bar.style, BarStyle::FullEdge);

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
mode = "dark"
accent = "#6750A4"
font_family = "sans-serif"
corner_radius_scale = 1.0

[bar]
position = "top"
style = "floating-capsule"
height = 48
padding = 8
widget_spacing = 6
[bar.margin]
horizontal = 16
vertical = 6
[bar.widgets]
start = ["launcher"]
center = ["clock"]
end = ["settings"]

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
}
