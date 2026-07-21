use serde::{Deserialize, Serialize};

/// Shilpo Desktop Shell configuration settings.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ShellConfig {
    pub bar: BarConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BarConfig {
    pub position: BarPosition,
    pub height: u32,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            position: BarPosition::Top,
            height: 48,
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
