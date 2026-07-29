use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use shilpo_ext::CanonicalId;
use std::{borrow::Cow, collections::BTreeMap, fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuiltinActionId {
    ToggleLauncher,
    ToggleControlCenter,
    ToggleBar,
    ToggleOverview,
    FocusWorkspace,
    FocusWindow,
    CloseWindow,
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

impl BuiltinActionId {
    const ALL: &'static [Self] = &[
        Self::ToggleLauncher,
        Self::ToggleControlCenter,
        Self::ToggleBar,
        Self::ToggleOverview,
        Self::FocusWorkspace,
        Self::FocusWindow,
        Self::CloseWindow,
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

    fn name(self) -> &'static str {
        match self {
            Self::ToggleLauncher => "toggle_launcher",
            Self::ToggleControlCenter => "toggle_control_center",
            Self::ToggleBar => "toggle_bar",
            Self::ToggleOverview => "toggle_overview",
            Self::FocusWorkspace => "focus_workspace",
            Self::FocusWindow => "focus_window",
            Self::CloseWindow => "close_window",
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

/// Stable, namespaced identifier for built-in and extension-provided actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(Cow<'static, str>);

#[allow(non_upper_case_globals)]
impl ActionId {
    pub const ToggleLauncher: Self = Self(Cow::Borrowed("builtin:toggle_launcher"));
    pub const ToggleControlCenter: Self = Self(Cow::Borrowed("builtin:toggle_control_center"));
    pub const ToggleBar: Self = Self(Cow::Borrowed("builtin:toggle_bar"));
    pub const ToggleOverview: Self = Self(Cow::Borrowed("builtin:toggle_overview"));
    pub const FocusWorkspace: Self = Self(Cow::Borrowed("builtin:focus_workspace"));
    pub const FocusWindow: Self = Self(Cow::Borrowed("builtin:focus_window"));
    pub const CloseWindow: Self = Self(Cow::Borrowed("builtin:close_window"));
    pub const CreateWorkspace: Self = Self(Cow::Borrowed("builtin:create_workspace"));
    pub const MoveWindowToWorkspace: Self = Self(Cow::Borrowed("builtin:move_window_to_workspace"));
    pub const ReloadConfig: Self = Self(Cow::Borrowed("builtin:reload_config"));
    pub const Quit: Self = Self(Cow::Borrowed("builtin:quit"));
    pub const VolumeUp: Self = Self(Cow::Borrowed("builtin:volume_up"));
    pub const VolumeDown: Self = Self(Cow::Borrowed("builtin:volume_down"));
    pub const VolumeMute: Self = Self(Cow::Borrowed("builtin:volume_mute"));
    pub const BrightnessUp: Self = Self(Cow::Borrowed("builtin:brightness_up"));
    pub const BrightnessDown: Self = Self(Cow::Borrowed("builtin:brightness_down"));
    pub const TakeScreenshot: Self = Self(Cow::Borrowed("builtin:take_screenshot"));
    pub const RecordScreen: Self = Self(Cow::Borrowed("builtin:record_screen"));

    pub fn name(&self) -> &str {
        self.0
            .strip_prefix("builtin:")
            .or_else(|| self.0.rsplit_once('/').map(|(_, name)| name))
            .unwrap_or(&self.0)
    }

    pub fn extension(id: CanonicalId) -> Self {
        Self(Cow::Owned(format!("ext:{id}")))
    }

    pub fn extension_id(&self) -> Option<CanonicalId> {
        self.0.strip_prefix("ext:")?.parse().ok()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ActionId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some(name) = value.strip_prefix("builtin:") {
            return BuiltinActionId::ALL
                .iter()
                .copied()
                .find(|id| id.name() == name)
                .map(action_id)
                .ok_or_else(|| format!("unknown built-in action '{name}'"));
        }
        let id = value
            .strip_prefix("ext:")
            .ok_or_else(|| format!("action ID '{value}' is missing its namespace"))?
            .parse()
            .map_err(|error: shilpo_ext::ManifestError| error.to_string())?;
        Ok(Self::extension(id))
    }
}

impl Serialize for ActionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ActionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

/// Category classification for shell actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    Navigation,
    Overlay,
    System,
    Extension,
}

/// Typed action invocation payload carrying optional parameters (e.g. workspace target).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionInvocation {
    ToggleLauncher,
    ToggleControlCenter,
    ToggleBar,
    ToggleOverview,
    FocusWorkspace(u64),
    FocusWindow(u64),
    CloseWindow(u64),
    CreateWorkspace,
    MoveWindowToWorkspace {
        window_id: u64,
        workspace_id: u64,
    },
    ReloadConfig,
    Quit,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    BrightnessUp,
    BrightnessDown,
    TakeScreenshot,
    RecordScreen,
    Extension {
        id: CanonicalId,
        payload: Option<serde_json::Value>,
    },
}

impl ActionInvocation {
    pub fn id(&self) -> ActionId {
        match self {
            Self::ToggleLauncher => ActionId::ToggleLauncher,
            Self::ToggleControlCenter => ActionId::ToggleControlCenter,
            Self::ToggleBar => ActionId::ToggleBar,
            Self::ToggleOverview => ActionId::ToggleOverview,
            Self::FocusWorkspace(_) => ActionId::FocusWorkspace,
            Self::FocusWindow(_) => ActionId::FocusWindow,
            Self::CloseWindow(_) => ActionId::CloseWindow,
            Self::CreateWorkspace => ActionId::CreateWorkspace,
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
            Self::Extension { id, .. } => ActionId::extension(id.clone()),
        }
    }

    /// Verifies that this invocation matches the provided action descriptor.
    pub fn matches_descriptor(&self, descriptor: &ActionDescriptor) -> bool {
        self.id() == descriptor.id
    }
}

/// Result outcome of executing an action invocation.
pub enum ActionResult {
    Immediate,
    Compositor(shilpo_services::CommandTicket),
}

impl From<ActionId> for ActionInvocation {
    fn from(id: ActionId) -> Self {
        if id == ActionId::ToggleLauncher {
            Self::ToggleLauncher
        } else if id == ActionId::ToggleControlCenter {
            Self::ToggleControlCenter
        } else if id == ActionId::ToggleBar {
            Self::ToggleBar
        } else if id == ActionId::ToggleOverview {
            Self::ToggleOverview
        } else if id == ActionId::FocusWorkspace {
            Self::FocusWorkspace(1)
        } else if id == ActionId::FocusWindow {
            Self::FocusWindow(1)
        } else if id == ActionId::CloseWindow {
            Self::CloseWindow(1)
        } else if id == ActionId::CreateWorkspace {
            Self::CreateWorkspace
        } else if id == ActionId::MoveWindowToWorkspace {
            Self::MoveWindowToWorkspace {
                window_id: 0,
                workspace_id: 1,
            }
        } else if id == ActionId::ReloadConfig {
            Self::ReloadConfig
        } else if id == ActionId::Quit {
            Self::Quit
        } else if id == ActionId::VolumeUp {
            Self::VolumeUp
        } else if id == ActionId::VolumeDown {
            Self::VolumeDown
        } else if id == ActionId::VolumeMute {
            Self::VolumeMute
        } else if id == ActionId::BrightnessUp {
            Self::BrightnessUp
        } else if id == ActionId::BrightnessDown {
            Self::BrightnessDown
        } else if id == ActionId::TakeScreenshot {
            Self::TakeScreenshot
        } else if id == ActionId::RecordScreen {
            Self::RecordScreen
        } else if let Some(id) = id.extension_id() {
            Self::Extension { id, payload: None }
        } else {
            unreachable!("ActionId construction validates its namespace")
        }
    }
}

/// Metadata descriptor for a registered shell action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionDescriptor {
    pub id: ActionId,
    pub name: String,
    pub label: String,
    pub category: ActionCategory,
    pub enabled: bool,
}

/// Registry of authoritative shell actions and enablement predicates.
#[derive(Debug, Clone)]
pub struct ActionRegistry {
    descriptors: BTreeMap<ActionId, ActionDescriptor>,
}

impl Default for ActionRegistry {
    fn default() -> Self {
        let descriptors = BuiltinActionId::ALL
            .iter()
            .copied()
            .map(builtin_descriptor)
            .map(|descriptor| (descriptor.id.clone(), descriptor))
            .collect();
        Self { descriptors }
    }
}

impl ActionRegistry {
    pub fn all(&self) -> Vec<ActionDescriptor> {
        self.descriptors.values().cloned().collect()
    }

    pub fn descriptor(&self, id: &ActionId) -> Option<&ActionDescriptor> {
        self.descriptors.get(id)
    }

    pub fn descriptor_mut(&mut self, id: &ActionId) -> Option<&mut ActionDescriptor> {
        self.descriptors.get_mut(id)
    }

    pub fn register_extension(
        &mut self,
        id: CanonicalId,
        name: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<ActionId, String> {
        let id = ActionId::extension(id);
        if self.descriptors.contains_key(&id) {
            return Err(format!("action '{id}' is already registered"));
        }
        self.descriptors.insert(
            id.clone(),
            ActionDescriptor {
                id: id.clone(),
                name: name.into(),
                label: label.into(),
                category: ActionCategory::Extension,
                enabled: true,
            },
        );
        Ok(id)
    }

    pub fn unregister_extension(&mut self, id: &CanonicalId) -> Option<ActionDescriptor> {
        self.descriptors.remove(&ActionId::extension(id.clone()))
    }
}

fn builtin_descriptor(id: BuiltinActionId) -> ActionDescriptor {
    let (label, category) = match id {
        BuiltinActionId::ToggleLauncher => ("Toggle Launcher Overlay", ActionCategory::Overlay),
        BuiltinActionId::ToggleControlCenter => ("Toggle Control Center", ActionCategory::Overlay),
        BuiltinActionId::ToggleBar => ("Toggle Desktop Bar", ActionCategory::System),
        BuiltinActionId::ToggleOverview => ("Toggle Workspace Overview", ActionCategory::Overlay),
        BuiltinActionId::FocusWorkspace => {
            ("Focus Compositor Workspace", ActionCategory::Navigation)
        }
        BuiltinActionId::FocusWindow => ("Focus Window", ActionCategory::Navigation),
        BuiltinActionId::CloseWindow => ("Close Window", ActionCategory::Navigation),
        BuiltinActionId::CreateWorkspace => ("Create Workspace", ActionCategory::Navigation),
        BuiltinActionId::MoveWindowToWorkspace => (
            "Move Focused Window to Workspace",
            ActionCategory::Navigation,
        ),
        BuiltinActionId::ReloadConfig => ("Reload Shell Configuration", ActionCategory::System),
        BuiltinActionId::Quit => ("Quit Shell Runtime", ActionCategory::System),
        BuiltinActionId::VolumeUp => ("Increase Volume", ActionCategory::System),
        BuiltinActionId::VolumeDown => ("Decrease Volume", ActionCategory::System),
        BuiltinActionId::VolumeMute => ("Mute Volume", ActionCategory::System),
        BuiltinActionId::BrightnessUp => ("Increase Brightness", ActionCategory::System),
        BuiltinActionId::BrightnessDown => ("Decrease Brightness", ActionCategory::System),
        BuiltinActionId::TakeScreenshot => ("Take Screenshot", ActionCategory::System),
        BuiltinActionId::RecordScreen => ("Record Screen Video", ActionCategory::System),
    };
    let id = action_id(id);
    ActionDescriptor {
        name: id.name().to_owned(),
        id,
        label: label.to_owned(),
        category,
        enabled: true,
    }
}

fn action_id(id: BuiltinActionId) -> ActionId {
    match id {
        BuiltinActionId::ToggleLauncher => ActionId::ToggleLauncher,
        BuiltinActionId::ToggleControlCenter => ActionId::ToggleControlCenter,
        BuiltinActionId::ToggleBar => ActionId::ToggleBar,
        BuiltinActionId::ToggleOverview => ActionId::ToggleOverview,
        BuiltinActionId::FocusWorkspace => ActionId::FocusWorkspace,
        BuiltinActionId::FocusWindow => ActionId::FocusWindow,
        BuiltinActionId::CloseWindow => ActionId::CloseWindow,
        BuiltinActionId::CreateWorkspace => ActionId::CreateWorkspace,
        BuiltinActionId::MoveWindowToWorkspace => ActionId::MoveWindowToWorkspace,
        BuiltinActionId::ReloadConfig => ActionId::ReloadConfig,
        BuiltinActionId::Quit => ActionId::Quit,
        BuiltinActionId::VolumeUp => ActionId::VolumeUp,
        BuiltinActionId::VolumeDown => ActionId::VolumeDown,
        BuiltinActionId::VolumeMute => ActionId::VolumeMute,
        BuiltinActionId::BrightnessUp => ActionId::BrightnessUp,
        BuiltinActionId::BrightnessDown => ActionId::BrightnessDown,
        BuiltinActionId::TakeScreenshot => ActionId::TakeScreenshot,
        BuiltinActionId::RecordScreen => ActionId::RecordScreen,
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
        self.bindings.get(shortcut).cloned()
    }

    pub fn register_with_override(
        &mut self,
        shortcut: Shortcut,
        action: ActionId,
    ) -> Option<ActionId> {
        self.bindings.insert(shortcut, action)
    }

    pub fn find_conflict(&self, shortcut: &Shortcut, action: ActionId) -> Option<ActionId> {
        if let Some(existing) = self.bindings.get(shortcut)
            && existing != &action
        {
            Some(existing.clone())
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
        let descriptors = ActionRegistry::default().all();
        assert_eq!(descriptors.len(), BuiltinActionId::ALL.len());

        for desc in &descriptors {
            assert!(!desc.name.is_empty());
            assert!(!desc.label.is_empty());
            assert!(desc.enabled);

            let json = serde_json::to_string(desc).unwrap();
            assert!(json.contains(&desc.name));
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

        let descriptors = ActionRegistry::default().all();
        let fw_desc = descriptors
            .iter()
            .find(|d| d.id == ActionId::FocusWorkspace)
            .unwrap();
        assert!(inv.matches_descriptor(fw_desc));

        let json = serde_json::to_string(&inv).unwrap();
        let deserialized: ActionInvocation = serde_json::from_str(&json).unwrap();
        assert_eq!(inv, deserialized);
    }

    #[test]
    fn extension_actions_use_namespaced_ids_and_reject_duplicates() {
        let canonical: CanonicalId = "io.github.alice.world-clock/refresh".parse().unwrap();
        let mut registry = ActionRegistry::default();
        let id = registry
            .register_extension(canonical.clone(), "refresh", "Refresh World Clock")
            .unwrap();

        assert_eq!(id.to_string(), "ext:io.github.alice.world-clock/refresh");
        assert_eq!(id.to_string().parse::<ActionId>().unwrap(), id);
        assert!(registry.descriptor(&id).is_some());
        assert!(
            registry
                .register_extension(canonical, "refresh", "Duplicate")
                .is_err()
        );
        assert!("refresh".parse::<ActionId>().is_err());
    }
}
