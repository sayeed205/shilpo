use crate::config::types::ShellConfig;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigChangeSet {
    pub theme: bool,
    pub bar: bool,
    pub desktop: bool,
    pub extensions: bool,
    pub outputs: bool,
    pub startup: bool,
    pub capture: bool,
    pub clipboard: bool,
    pub clock_format: bool,
    pub temperature_unit: bool,
    pub locale: bool,
}

impl ConfigChangeSet {
    pub fn is_empty(&self) -> bool {
        !self.theme
            && !self.bar
            && !self.desktop
            && !self.extensions
            && !self.outputs
            && !self.startup
            && !self.capture
            && !self.clipboard
            && !self.clock_format
            && !self.temperature_unit
            && !self.locale
    }

    pub fn all() -> Self {
        Self {
            theme: true,
            bar: true,
            desktop: true,
            extensions: true,
            outputs: true,
            startup: true,
            capture: true,
            clipboard: true,
            clock_format: true,
            temperature_unit: true,
            locale: true,
        }
    }

    pub fn compute(old: &ShellConfig, new: &ShellConfig) -> Self {
        Self {
            theme: old.theme != new.theme,
            bar: old.bar != new.bar,
            desktop: old.desktop != new.desktop,
            extensions: old.extensions != new.extensions,
            outputs: old.outputs != new.outputs,
            startup: old.startup != new.startup,
            capture: old.capture != new.capture,
            clipboard: old.clipboard != new.clipboard,
            clock_format: old.clock_format != new.clock_format,
            temperature_unit: old.temperature_unit != new.temperature_unit,
            locale: old.locale != new.locale,
        }
    }
}
