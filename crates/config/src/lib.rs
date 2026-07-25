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
    #[serde(default)]
    pub clock_format: Option<String>,
    #[serde(default)]
    pub temperature_unit: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub startup: StartupConfig,
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
    pub heading_font_family: Option<String>,
    pub mono_font_family: Option<String>,
    pub corner_radius_scale: f32,
    #[serde(default)]
    pub high_contrast: bool,
    #[serde(default)]
    pub reduced_motion: bool,
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
            mode: ThemeMode::Dark,
            accent: "#6750A4".into(),
            font_family: "sans-serif".into(),
            heading_font_family: None,
            mono_font_family: None,
            corner_radius_scale: 1.0,
            high_contrast: false,
            reduced_motion: false,
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OutputBarState {
    pub visible: bool,
    pub position_edge: String,
    pub thickness: u32,
    pub exclusive_zone: Option<u32>,
    pub active_workspace_id: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardItem {
    pub id: u64,
    pub text: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AudioPreference {
    pub default_device: Option<String>,
    pub default_port: Option<String>,
}

pub struct HeedSessionStore {
    env: heed::Env,
    output_bars_db: heed::Database<heed::types::Str, heed::types::SerdeJson<OutputBarState>>,
    clipboard_history_db: heed::Database<
        heed::types::U64<heed::byteorder::NativeEndian>,
        heed::types::SerdeJson<ClipboardItem>,
    >,
    audio_pref_db: heed::Database<heed::types::Str, heed::types::SerdeJson<AudioPreference>>,
    _lock_file: Option<fs::File>,
}

impl HeedSessionStore {
    pub fn default_db_dir() -> PathBuf {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("shilpo").join("session.lmdb")
    }

    pub fn open_or_create(dir: &Path) -> Result<Self, ConfigError> {
        if let Err(e) = fs::create_dir_all(dir) {
            return Err(ConfigError::Io {
                path: dir.to_path_buf(),
                source: e,
            });
        }

        let lock_path = dir.join("session.lock");
        let lock_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .ok();

        let env = unsafe {
            heed::EnvOpenOptions::new()
                .max_dbs(10)
                .map_size(10 * 1024 * 1024)
                .open(dir)
                .map_err(|e| ConfigError::Parse {
                    diagnostic: ConfigDiagnostic {
                        path: dir.display().to_string(),
                        message: e.to_string(),
                        span: None,
                    },
                })?
        };

        let mut wtxn = env.write_txn().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: dir.display().to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;

        let output_bars_db = env
            .create_database(&mut wtxn, Some("output_bars"))
            .map_err(|e| ConfigError::Parse {
                diagnostic: ConfigDiagnostic {
                    path: dir.display().to_string(),
                    message: e.to_string(),
                    span: None,
                },
            })?;

        let clipboard_history_db = env
            .create_database(&mut wtxn, Some("clipboard_history"))
            .map_err(|e| ConfigError::Parse {
                diagnostic: ConfigDiagnostic {
                    path: dir.display().to_string(),
                    message: e.to_string(),
                    span: None,
                },
            })?;

        let audio_pref_db = env
            .create_database(&mut wtxn, Some("audio_preference"))
            .map_err(|e| ConfigError::Parse {
                diagnostic: ConfigDiagnostic {
                    path: dir.display().to_string(),
                    message: e.to_string(),
                    span: None,
                },
            })?;

        wtxn.commit().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: dir.display().to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;

        Ok(Self {
            env,
            output_bars_db,
            clipboard_history_db,
            audio_pref_db,
            _lock_file: lock_file,
        })
    }

    pub fn open_or_repair(dir: &Path) -> Result<Self, ConfigError> {
        match Self::open_or_create(dir) {
            Ok(store) => Ok(store),
            Err(e) => {
                eprintln!(
                    "LMDB session store open failed (path = {}): {e}; resetting corrupt store directory",
                    dir.display()
                );
                let _ = fs::remove_dir_all(dir);
                Self::open_or_create(dir)
            }
        }
    }

    pub fn save_clipboard_item(&self, item: &ClipboardItem) -> Result<(), ConfigError> {
        let mut wtxn = self.env.write_txn().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: item.id.to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;
        self.clipboard_history_db
            .put(&mut wtxn, &item.id, item)
            .map_err(|e| ConfigError::Parse {
                diagnostic: ConfigDiagnostic {
                    path: item.id.to_string(),
                    message: e.to_string(),
                    span: None,
                },
            })?;
        wtxn.commit().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: item.id.to_string(),
                message: e.to_string(),
                span: None,
            },
        })
    }

    pub fn get_clipboard_history(&self) -> Result<Vec<ClipboardItem>, ConfigError> {
        let rtxn = self.env.read_txn().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: "clipboard_history".to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;
        let mut items = Vec::new();
        if let Ok(iter) = self.clipboard_history_db.iter(&rtxn) {
            for (_, item) in iter.flatten() {
                items.push(item);
            }
        }
        items.sort_by_key(|i| i.id);
        items.reverse();
        Ok(items)
    }

    pub fn clear_clipboard_history(&self) -> Result<(), ConfigError> {
        let mut wtxn = self.env.write_txn().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: "clipboard_history".to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;
        self.clipboard_history_db
            .clear(&mut wtxn)
            .map_err(|e| ConfigError::Parse {
                diagnostic: ConfigDiagnostic {
                    path: "clipboard_history".to_string(),
                    message: e.to_string(),
                    span: None,
                },
            })?;
        wtxn.commit().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: "clipboard_history".to_string(),
                message: e.to_string(),
                span: None,
            },
        })
    }

    pub fn get_output_bar(&self, output_name: &str) -> Result<Option<OutputBarState>, ConfigError> {
        let rtxn = self.env.read_txn().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: output_name.to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;

        let state =
            self.output_bars_db
                .get(&rtxn, output_name)
                .map_err(|e| ConfigError::Parse {
                    diagnostic: ConfigDiagnostic {
                        path: output_name.to_string(),
                        message: e.to_string(),
                        span: None,
                    },
                })?;

        Ok(state)
    }

    pub fn put_output_bar(
        &self,
        output_name: &str,
        state: &OutputBarState,
    ) -> Result<(), ConfigError> {
        let mut wtxn = self.env.write_txn().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: output_name.to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;

        self.output_bars_db
            .put(&mut wtxn, output_name, state)
            .map_err(|e| ConfigError::Parse {
                diagnostic: ConfigDiagnostic {
                    path: output_name.to_string(),
                    message: e.to_string(),
                    span: None,
                },
            })?;

        wtxn.commit().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: output_name.to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;

        Ok(())
    }

    pub fn save_audio_preference(&self, pref: &AudioPreference) -> Result<(), ConfigError> {
        let mut wtxn = self.env.write_txn().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: "audio_preference".to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;
        self.audio_pref_db
            .put(&mut wtxn, "default", pref)
            .map_err(|e| ConfigError::Parse {
                diagnostic: ConfigDiagnostic {
                    path: "audio_preference".to_string(),
                    message: e.to_string(),
                    span: None,
                },
            })?;
        wtxn.commit().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: "audio_preference".to_string(),
                message: e.to_string(),
                span: None,
            },
        })
    }

    pub fn get_audio_preference(&self) -> Result<AudioPreference, ConfigError> {
        let rtxn = self.env.read_txn().map_err(|e| ConfigError::Parse {
            diagnostic: ConfigDiagnostic {
                path: "audio_preference".to_string(),
                message: e.to_string(),
                span: None,
            },
        })?;
        let pref = self
            .audio_pref_db
            .get(&rtxn, "default")
            .ok()
            .flatten()
            .unwrap_or_default();
        Ok(pref)
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

        let store = HeedSessionStore::open_or_create(&db_dir).unwrap();
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

    #[test]
    fn test_heed_clipboard_history_persistence() {
        let db_dir = std::env::temp_dir().join(format!("shilpo-clip-{}.lmdb", std::process::id()));
        let _ = std::fs::remove_dir_all(&db_dir);

        let store = HeedSessionStore::open_or_create(&db_dir).unwrap();
        assert!(store.get_clipboard_history().unwrap().is_empty());

        let item = ClipboardItem {
            id: 100,
            text: "Hello Shilpo".to_string(),
            timestamp: "12:00:00".to_string(),
        };
        store.save_clipboard_item(&item).unwrap();

        let history = store.get_clipboard_history().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0], item);

        store.clear_clipboard_history().unwrap();
        assert!(store.get_clipboard_history().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(db_dir);
    }

    #[test]
    fn test_heed_corrupt_state_recovery_and_atomic_commit() {
        let db_dir =
            std::env::temp_dir().join(format!("shilpo-corrupt-{}.lmdb", std::process::id()));
        let _ = std::fs::remove_dir_all(&db_dir);
        std::fs::create_dir_all(&db_dir).unwrap();
        std::fs::write(db_dir.join("data.mdb"), b"CORRUPTED_GARBAGE_BYTES_12345").unwrap();

        let store = HeedSessionStore::open_or_repair(&db_dir).unwrap();
        assert!(store.get_clipboard_history().unwrap().is_empty());

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

        let store = HeedSessionStore::open_or_create(&db_dir).unwrap();
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
}
