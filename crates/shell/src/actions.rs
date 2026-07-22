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
}
