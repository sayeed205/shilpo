use serde::{Deserialize, Serialize};

/// Stable identifier for registered shell actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionId {
    ToggleLauncher,
    ToggleControlCenter,
    ToggleBar,
    FocusWorkspace,
    CreateWorkspace,
    MoveWindowToWorkspace,
    ReloadConfig,
    Quit,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    BrightnessUp,
    BrightnessDown,
    TakeScreenshot,
    RecordScreen,
}

impl ActionId {
    pub const ALL: &'static [Self] = &[
        Self::ToggleLauncher,
        Self::ToggleControlCenter,
        Self::ToggleBar,
        Self::FocusWorkspace,
        Self::CreateWorkspace,
        Self::MoveWindowToWorkspace,
        Self::ReloadConfig,
        Self::Quit,
        Self::VolumeUp,
        Self::VolumeDown,
        Self::VolumeMute,
        Self::BrightnessUp,
        Self::BrightnessDown,
        Self::TakeScreenshot,
        Self::RecordScreen,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::ToggleLauncher => "toggle_launcher",
            Self::ToggleControlCenter => "toggle_control_center",
            Self::ToggleBar => "toggle_bar",
            Self::FocusWorkspace => "focus_workspace",
            Self::CreateWorkspace => "create_workspace",
            Self::MoveWindowToWorkspace => "move_window_to_workspace",
            Self::ReloadConfig => "reload_config",
            Self::Quit => "quit",
            Self::VolumeUp => "volume_up",
            Self::VolumeDown => "volume_down",
            Self::VolumeMute => "volume_mute",
            Self::BrightnessUp => "brightness_up",
            Self::BrightnessDown => "brightness_down",
            Self::TakeScreenshot => "take_screenshot",
            Self::RecordScreen => "record_screen",
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

/// Typed action invocation payload carrying optional parameters (e.g. workspace target).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionInvocation {
    ToggleLauncher,
    ToggleControlCenter,
    ToggleBar,
    FocusWorkspace(u64),
    CreateWorkspace(Option<String>),
    MoveWindowToWorkspace { window_id: u64, workspace_id: u64 },
    ReloadConfig,
    Quit,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    BrightnessUp,
    BrightnessDown,
    TakeScreenshot,
    RecordScreen,
}

impl ActionInvocation {
    pub fn id(&self) -> ActionId {
        match self {
            Self::ToggleLauncher => ActionId::ToggleLauncher,
            Self::ToggleControlCenter => ActionId::ToggleControlCenter,
            Self::ToggleBar => ActionId::ToggleBar,
            Self::FocusWorkspace(_) => ActionId::FocusWorkspace,
            Self::CreateWorkspace(_) => ActionId::CreateWorkspace,
            Self::MoveWindowToWorkspace { .. } => ActionId::MoveWindowToWorkspace,
            Self::ReloadConfig => ActionId::ReloadConfig,
            Self::Quit => ActionId::Quit,
            Self::VolumeUp => ActionId::VolumeUp,
            Self::VolumeDown => ActionId::VolumeDown,
            Self::VolumeMute => ActionId::VolumeMute,
            Self::BrightnessUp => ActionId::BrightnessUp,
            Self::BrightnessDown => ActionId::BrightnessDown,
            Self::TakeScreenshot => ActionId::TakeScreenshot,
            Self::RecordScreen => ActionId::RecordScreen,
        }
    }

    /// Verifies that this invocation matches the provided action descriptor.
    pub fn matches_descriptor(&self, descriptor: &ActionDescriptor) -> bool {
        self.id() == descriptor.id
    }
}

impl From<ActionId> for ActionInvocation {
    fn from(id: ActionId) -> Self {
        match id {
            ActionId::ToggleLauncher => Self::ToggleLauncher,
            ActionId::ToggleControlCenter => Self::ToggleControlCenter,
            ActionId::ToggleBar => Self::ToggleBar,
            ActionId::FocusWorkspace => Self::FocusWorkspace(1),
            ActionId::CreateWorkspace => Self::CreateWorkspace(None),
            ActionId::MoveWindowToWorkspace => Self::MoveWindowToWorkspace {
                window_id: 0,
                workspace_id: 1,
            },
            ActionId::ReloadConfig => Self::ReloadConfig,
            ActionId::Quit => Self::Quit,
            ActionId::VolumeUp => Self::VolumeUp,
            ActionId::VolumeDown => Self::VolumeDown,
            ActionId::VolumeMute => Self::VolumeMute,
            ActionId::BrightnessUp => Self::BrightnessUp,
            ActionId::BrightnessDown => Self::BrightnessDown,
            ActionId::TakeScreenshot => Self::TakeScreenshot,
            ActionId::RecordScreen => Self::RecordScreen,
        }
    }
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
                ActionId::CreateWorkspace => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Create Workspace",
                    category: ActionCategory::Navigation,
                    enabled: true,
                },
                ActionId::MoveWindowToWorkspace => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Move Focused Window to Workspace",
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
                ActionId::VolumeUp => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Increase Volume",
                    category: ActionCategory::System,
                    enabled: true,
                },
                ActionId::VolumeDown => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Decrease Volume",
                    category: ActionCategory::System,
                    enabled: true,
                },
                ActionId::VolumeMute => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Mute Volume",
                    category: ActionCategory::System,
                    enabled: true,
                },
                ActionId::BrightnessUp => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Increase Brightness",
                    category: ActionCategory::System,
                    enabled: true,
                },
                ActionId::BrightnessDown => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Decrease Brightness",
                    category: ActionCategory::System,
                    enabled: true,
                },
                ActionId::TakeScreenshot => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Take Screenshot",
                    category: ActionCategory::System,
                    enabled: true,
                },
                ActionId::RecordScreen => ActionDescriptor {
                    id,
                    name: id.name(),
                    label: "Record Screen Video",
                    category: ActionCategory::System,
                    enabled: true,
                },
            })
            .collect()
    }
}

/// Representation of a parsed key combination shortcut (e.g. "super+space").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shortcut {
    pub modifiers: Vec<String>,
    pub key: String,
}

impl Shortcut {
    pub fn parse(spec: &str) -> Option<Self> {
        let parts: Vec<&str> = spec.split('+').map(|s| s.trim()).collect();
        if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
            return None;
        }
        let key = parts.last()?.to_lowercase();
        let mut modifiers: Vec<String> = parts[..parts.len() - 1]
            .iter()
            .map(|m| m.to_lowercase())
            .collect();
        modifiers.sort();
        Some(Self { modifiers, key })
    }

    pub fn to_spec(&self) -> String {
        if self.modifiers.is_empty() {
            self.key.clone()
        } else {
            format!("{}+{}", self.modifiers.join("+"), self.key)
        }
    }
}

/// Global lifecycle manager for shell keybindings and shortcut dispatch.
#[derive(Debug, Clone)]
pub struct KeybindingManager {
    bindings: std::collections::HashMap<Shortcut, ActionId>,
}

impl Default for KeybindingManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl KeybindingManager {
    pub fn new() -> Self {
        Self {
            bindings: std::collections::HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut mgr = Self::new();
        let defaults = [
            ("super+space", ActionId::ToggleLauncher),
            ("super+c", ActionId::ToggleControlCenter),
            ("super+b", ActionId::ToggleBar),
            ("super+shift+r", ActionId::ReloadConfig),
            ("super+shift+q", ActionId::Quit),
        ];

        for (spec, action) in defaults {
            if let Some(shortcut) = Shortcut::parse(spec) {
                let _ = mgr.register(shortcut, action);
            }
        }
        mgr
    }

    pub fn register(&mut self, shortcut: Shortcut, action: ActionId) -> Result<(), String> {
        if let Some(existing) = self.bindings.get(&shortcut)
            && *existing != action
        {
            return Err(format!(
                "shortcut conflict for '{}': already bound to '{:?}'",
                shortcut.to_spec(),
                existing
            ));
        }
        self.bindings.insert(shortcut, action);
        Ok(())
    }

    pub fn unregister(&mut self, shortcut: &Shortcut) -> Option<ActionId> {
        self.bindings.remove(shortcut)
    }

    pub fn action_for(&self, shortcut: &Shortcut) -> Option<ActionId> {
        self.bindings.get(shortcut).copied()
    }

    pub fn find_conflict(&self, shortcut: &Shortcut, action: ActionId) -> Option<ActionId> {
        if let Some(&existing) = self.bindings.get(shortcut)
            && existing != action
        {
            Some(existing)
        } else {
            None
        }
    }

    pub fn reset_to_defaults(&mut self) {
        *self = Self::with_defaults();
    }

    pub fn bindings(&self) -> &std::collections::HashMap<Shortcut, ActionId> {
        &self.bindings
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

    #[test]
    fn shortcut_parsing_and_keybinding_defaults() {
        let sc = Shortcut::parse("Super + Space").unwrap();
        assert_eq!(sc.modifiers, vec!["super"]);
        assert_eq!(sc.key, "space");
        assert_eq!(sc.to_spec(), "super+space");

        let mgr = KeybindingManager::with_defaults();
        assert_eq!(mgr.action_for(&sc), Some(ActionId::ToggleLauncher));

        let sc_bar = Shortcut::parse("super+b").unwrap();
        assert_eq!(mgr.action_for(&sc_bar), Some(ActionId::ToggleBar));
    }

    #[test]
    fn shortcut_conflict_detection() {
        let mut mgr = KeybindingManager::new();
        let sc = Shortcut::parse("super+c").unwrap();
        assert!(
            mgr.register(sc.clone(), ActionId::ToggleControlCenter)
                .is_ok()
        );

        // Conflict registration should fail with diagnostic
        let err = mgr.register(sc, ActionId::Quit).unwrap_err();
        assert!(err.contains("conflict"));
    }

    #[test]
    fn shortcut_conflict_query_and_reset_to_defaults() {
        let mut mgr = KeybindingManager::with_defaults();
        let sc = Shortcut::parse("super+space").unwrap();

        let conflict = mgr.find_conflict(&sc, ActionId::Quit);
        assert_eq!(conflict, Some(ActionId::ToggleLauncher));

        let _ = mgr.unregister(&sc);
        assert_eq!(mgr.action_for(&sc), None);

        mgr.reset_to_defaults();
        assert_eq!(mgr.action_for(&sc), Some(ActionId::ToggleLauncher));
    }

    #[test]
    fn action_invocation_payload_and_descriptor_matching() {
        let inv = ActionInvocation::FocusWorkspace(7);
        assert_eq!(inv.id(), ActionId::FocusWorkspace);

        let descriptors = ActionRegistry::all();
        let fw_desc = descriptors
            .iter()
            .find(|d| d.id == ActionId::FocusWorkspace)
            .unwrap();
        assert!(inv.matches_descriptor(fw_desc));

        let json = serde_json::to_string(&inv).unwrap();
        let deserialized: ActionInvocation = serde_json::from_str(&json).unwrap();
        assert_eq!(inv, deserialized);
    }
}
