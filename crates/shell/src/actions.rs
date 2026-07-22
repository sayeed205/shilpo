use serde::{Deserialize, Serialize};

/// Stable identifier for registered shell actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionId {
    ToggleLauncher,
    ToggleControlCenter,
    ToggleBar,
    FocusWorkspace,
    ReloadConfig,
    Quit,
}

impl ActionId {
    pub const ALL: &'static [Self] = &[
        Self::ToggleLauncher,
        Self::ToggleControlCenter,
        Self::ToggleBar,
        Self::FocusWorkspace,
        Self::ReloadConfig,
        Self::Quit,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ToggleLauncher => "toggle_launcher",
            Self::ToggleControlCenter => "toggle_control_center",
            Self::ToggleBar => "toggle_bar",
            Self::FocusWorkspace => "focus_workspace",
            Self::ReloadConfig => "reload_config",
            Self::Quit => "quit",
        }
    }
}

/// Category classification for shell actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    Navigation,
    Overlay,
    System,
}

/// Metadata descriptor for a registered shell action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionDescriptor {
    pub id: ActionId,
    pub name: &'static str,
    pub label: &'static str,
    pub category: ActionCategory,
    pub enabled: bool,
}

/// Registry of authoritative shell actions and enablement predicates.
#[derive(Debug, Clone, Default)]
pub struct ActionRegistry;

impl ActionRegistry {
    pub fn all() -> Vec<ActionDescriptor> {
        ActionId::ALL
            .iter()
            .map(|&id| match id {
                ActionId::ToggleLauncher => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Toggle Launcher Overlay",
                    category: ActionCategory::Overlay,
                    enabled: true,
                },
                ActionId::ToggleControlCenter => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Toggle Control Center",
                    category: ActionCategory::Overlay,
                    enabled: true,
                },
                ActionId::ToggleBar => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Toggle Desktop Bar",
                    category: ActionCategory::System,
                    enabled: true,
                },
                ActionId::FocusWorkspace => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Focus Compositor Workspace",
                    category: ActionCategory::Navigation,
                    enabled: true,
                },
                ActionId::ReloadConfig => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Reload Shell Configuration",
                    category: ActionCategory::System,
                    enabled: true,
                },
                ActionId::Quit => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Quit Shell Runtime",
                    category: ActionCategory::System,
                    enabled: true,
                },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_registry_completeness_and_serialization() {
        let descriptors = ActionRegistry::all();
        assert_eq!(descriptors.len(), ActionId::ALL.len());

        for desc in &descriptors {
            assert!(!desc.name.is_empty());
            assert!(!desc.label.is_empty());
            assert!(desc.enabled);

            let json = serde_json::to_string(desc).unwrap();
            assert!(json.contains(desc.name));
        }
    }
}
