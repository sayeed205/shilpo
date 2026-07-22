use serde::{Deserialize, Serialize};

/// Master Shilpo Desktop Shell configuration settings.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ShellConfig {
    pub theme: ThemeConfig,
    pub bar: BarConfig,
}

impl ShellConfig {
    pub fn load() -> Self {
        let home = std::env::var("HOME")
            .ok()
            .or_else(|| std::env::var("USER").ok().map(|u| format!("/home/{}", u)))
            .unwrap_or_else(|| ".".to_string());
        let path = std::path::PathBuf::from(home).join(".config/shilpo/config.toml");

        if !path.exists() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let default_config = ShellConfig::default();
            if let Ok(toml_str) = toml::to_string_pretty(&default_config) {
                let _ = std::fs::write(&path, toml_str);
            }
            return default_config;
        }

        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config) = toml::from_str(&content) {
                return config;
            } else {
                eprintln!(
                    "[shilpo-config] Warning: Failed to parse {}, falling back to default configuration",
                    path.display()
                );
            }
        }

        ShellConfig::default()
    }
}

/// Global UI Theme configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeConfig {
    pub mode: ThemeMode,
    pub primary_accent: String,
    pub font_family: String,
    pub corner_radius_scale: F32Eq,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            primary_accent: "#6750A4".into(),
            font_family: "sans-serif".into(),
            corner_radius_scale: F32Eq(1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    Auto,
}

/// Status Bar configuration settings.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BarConfig {
    pub position: BarPosition,
    pub style: BarStyle,
    pub height: u32,
    pub margin_h: u32,
    pub margin_v: u32,
    pub padding: u32,
    pub widget_spacing: u32,
    pub start: Vec<String>,
    pub center: Vec<String>,
    pub end: Vec<String>,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            position: BarPosition::Top,
            style: BarStyle::FloatingCapsule,
            height: 48,
            margin_h: 180,
            margin_v: 8,
            padding: 8,
            widget_spacing: 6,
            start: vec![
                "launcher".into(),
                "workspaces".into(),
                "active-window".into(),
            ],
            center: vec!["clock".into(), "media".into()],
            end: vec![
                "sysinfo".into(),
                "network".into(),
                "audio".into(),
                "battery".into(),
                "settings".into(),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum BarStyle {
    #[default]
    FloatingCapsule,
    FullEdge,
}

/// Helper wrapper for f32 comparison in Eq types.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct F32Eq(pub f32);

impl PartialEq for F32Eq {
    fn eq(&self, other: &Self) -> bool {
        (self.0 - other.0).abs() < f32::EPSILON
    }
}

impl Eq for F32Eq {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_serialization() {
        let default_config = ShellConfig::default();
        let serialized = toml::to_string(&default_config).unwrap();
        let deserialized: ShellConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(default_config, deserialized);
    }
}
