use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use shilpo_ext_api::{CanonicalId, IdError};
use std::{borrow::Cow, collections::BTreeMap, fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuiltinActionId {
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
}

impl BuiltinActionId {
    const ALL: &'static [Self] = &[
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
    ];

    fn name(self) -> &'static str {
        match self {
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
        }
    }

    pub fn input_requirement(self) -> ActionInputRequirement {
        match self {
            Self::ToggleBar
            | Self::ToggleOverview
            | Self::CreateWorkspace
            | Self::ReloadConfig
            | Self::Quit
            | Self::VolumeUp
            | Self::VolumeDown
            | Self::VolumeMute
            | Self::BrightnessUp
            | Self::BrightnessDown
            | Self::TakeScreenshot => ActionInputRequirement::NoInput,
            Self::FocusWorkspace => ActionInputRequirement::WorkspaceId,
            Self::FocusWindow | Self::CloseWindow => ActionInputRequirement::WindowId,
            Self::MoveWindowToWorkspace => ActionInputRequirement::WindowAndWorkspace,
        }
    }
}

/// Input requirement classification for shell actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionInputRequirement {
    NoInput,
    WorkspaceId,
    WindowId,
    WindowAndWorkspace,
    OptionalJson,
}

impl ActionInputRequirement {
    pub fn can_invoke_without_input(self) -> bool {
        matches!(self, Self::NoInput | Self::OptionalJson)
    }
}

/// Stable, namespaced identifier for built-in and extension-provided actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(Cow<'static, str>);

#[allow(non_upper_case_globals)]
impl ActionId {
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
            .map_err(|error: IdError| error.to_string())?;
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
    Extension {
        id: CanonicalId,
        payload: Option<serde_json::Value>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceActionInput {
    workspace_id: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowActionInput {
    window_id: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowWorkspaceActionInput {
    window_id: u64,
    workspace_id: u64,
}

impl ActionInvocation {
    pub fn id(&self) -> ActionId {
        match self {
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
            Self::Extension { id, .. } => ActionId::extension(id.clone()),
        }
    }

    pub fn from_id_and_payload(
        id: ActionId,
        payload: Option<serde_json::Value>,
    ) -> Result<Self, String> {
        if let Some(ext_id) = id.extension_id() {
            return Ok(Self::Extension {
                id: ext_id,
                payload,
            });
        }

        let builtin = BuiltinActionId::ALL
            .iter()
            .copied()
            .find(|b| action_id(*b) == id)
            .ok_or_else(|| format!("unknown action ID '{id}'"))?;

        match builtin.input_requirement() {
            ActionInputRequirement::NoInput => match payload {
                None | Some(serde_json::Value::Null) => match builtin {
                    BuiltinActionId::ToggleBar => Ok(Self::ToggleBar),
                    BuiltinActionId::ToggleOverview => Ok(Self::ToggleOverview),
                    BuiltinActionId::CreateWorkspace => Ok(Self::CreateWorkspace),
                    BuiltinActionId::ReloadConfig => Ok(Self::ReloadConfig),
                    BuiltinActionId::Quit => Ok(Self::Quit),
                    BuiltinActionId::VolumeUp => Ok(Self::VolumeUp),
                    BuiltinActionId::VolumeDown => Ok(Self::VolumeDown),
                    BuiltinActionId::VolumeMute => Ok(Self::VolumeMute),
                    BuiltinActionId::BrightnessUp => Ok(Self::BrightnessUp),
                    BuiltinActionId::BrightnessDown => Ok(Self::BrightnessDown),
                    BuiltinActionId::TakeScreenshot => Ok(Self::TakeScreenshot),
                    _ => unreachable!(),
                },
                Some(_) => Err(format!("action '{id}' does not accept input parameters")),
            },
            ActionInputRequirement::WorkspaceId => {
                let value = payload
                    .ok_or_else(|| format!("action '{id}' requires a 'workspace_id' parameter"))?;
                if value.is_null() {
                    return Err(format!("action '{id}' requires a 'workspace_id' parameter"));
                }
                let input: WorkspaceActionInput = serde_json::from_value(value)
                    .map_err(|_| format!("action '{id}' requires a 'workspace_id' parameter"))?;
                match builtin {
                    BuiltinActionId::FocusWorkspace => Ok(Self::FocusWorkspace(input.workspace_id)),
                    _ => unreachable!(),
                }
            }
            ActionInputRequirement::WindowId => {
                let value = payload
                    .ok_or_else(|| format!("action '{id}' requires a 'window_id' parameter"))?;
                if value.is_null() {
                    return Err(format!("action '{id}' requires a 'window_id' parameter"));
                }
                let input: WindowActionInput = serde_json::from_value(value)
                    .map_err(|_| format!("action '{id}' requires a 'window_id' parameter"))?;
                match builtin {
                    BuiltinActionId::FocusWindow => Ok(Self::FocusWindow(input.window_id)),
                    BuiltinActionId::CloseWindow => Ok(Self::CloseWindow(input.window_id)),
                    _ => unreachable!(),
                }
            }
            ActionInputRequirement::WindowAndWorkspace => {
                let value = payload.ok_or_else(|| {
                    format!("action '{id}' requires 'window_id' and 'workspace_id' parameters")
                })?;
                if value.is_null() {
                    return Err(format!(
                        "action '{id}' requires 'window_id' and 'workspace_id' parameters"
                    ));
                }
                let input: WindowWorkspaceActionInput =
                    serde_json::from_value(value).map_err(|_| {
                        format!("action '{id}' requires 'window_id' and 'workspace_id' parameters")
                    })?;
                match builtin {
                    BuiltinActionId::MoveWindowToWorkspace => Ok(Self::MoveWindowToWorkspace {
                        window_id: input.window_id,
                        workspace_id: input.workspace_id,
                    }),
                    _ => unreachable!(),
                }
            }
            ActionInputRequirement::OptionalJson => unreachable!(),
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

/// Metadata descriptor for a registered shell action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionDescriptor {
    pub id: ActionId,
    pub name: String,
    pub label: String,
    pub category: ActionCategory,
    pub input: ActionInputRequirement,
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
                input: ActionInputRequirement::OptionalJson,
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
    };
    let id_action = action_id(id);
    ActionDescriptor {
        name: id_action.name().to_owned(),
        id: id_action,
        label: label.to_owned(),
        category,
        input: id.input_requirement(),
        enabled: true,
    }
}

fn action_id(id: BuiltinActionId) -> ActionId {
    match id {
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
            ("super+space", ActionId::ToggleOverview),
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

    pub fn shortcut_for(&self, action: &ActionId) -> Option<Shortcut> {
        self.bindings
            .iter()
            .find_map(|(s, a)| (a == action).then(|| s.clone()))
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
            assert!(json.contains("input"));
        }
    }

    #[test]
    fn builtin_and_extension_input_requirements_mapping() {
        let registry = ActionRegistry::default();
        let find_req = |id: &ActionId| registry.descriptor(id).unwrap().input;

        assert_eq!(
            find_req(&ActionId::ToggleOverview),
            ActionInputRequirement::NoInput
        );
        assert_eq!(
            find_req(&ActionId::ToggleBar),
            ActionInputRequirement::NoInput
        );
        assert_eq!(
            find_req(&ActionId::ToggleOverview),
            ActionInputRequirement::NoInput
        );
        assert_eq!(
            find_req(&ActionId::CreateWorkspace),
            ActionInputRequirement::NoInput
        );
        assert_eq!(
            find_req(&ActionId::ReloadConfig),
            ActionInputRequirement::NoInput
        );
        assert_eq!(find_req(&ActionId::Quit), ActionInputRequirement::NoInput);
        assert_eq!(
            find_req(&ActionId::VolumeUp),
            ActionInputRequirement::NoInput
        );
        assert_eq!(
            find_req(&ActionId::VolumeDown),
            ActionInputRequirement::NoInput
        );
        assert_eq!(
            find_req(&ActionId::VolumeMute),
            ActionInputRequirement::NoInput
        );
        assert_eq!(
            find_req(&ActionId::BrightnessUp),
            ActionInputRequirement::NoInput
        );
        assert_eq!(
            find_req(&ActionId::BrightnessDown),
            ActionInputRequirement::NoInput
        );
        assert_eq!(
            find_req(&ActionId::TakeScreenshot),
            ActionInputRequirement::NoInput
        );

        assert_eq!(
            find_req(&ActionId::FocusWorkspace),
            ActionInputRequirement::WorkspaceId
        );
        assert_eq!(
            find_req(&ActionId::FocusWindow),
            ActionInputRequirement::WindowId
        );
        assert_eq!(
            find_req(&ActionId::CloseWindow),
            ActionInputRequirement::WindowId
        );
        assert_eq!(
            find_req(&ActionId::MoveWindowToWorkspace),
            ActionInputRequirement::WindowAndWorkspace
        );

        let canonical: CanonicalId = "io.github.alice.world-clock/refresh".parse().unwrap();
        let mut reg_ext = ActionRegistry::default();
        let ext_id = reg_ext
            .register_extension(canonical, "refresh", "Refresh World Clock")
            .unwrap();
        assert_eq!(
            reg_ext.descriptor(&ext_id).unwrap().input,
            ActionInputRequirement::OptionalJson
        );
    }

    #[test]
    fn from_id_and_payload_no_input_actions() {
        let inv_none =
            ActionInvocation::from_id_and_payload(ActionId::ToggleOverview, None).unwrap();
        assert_eq!(inv_none, ActionInvocation::ToggleOverview);

        let inv_null = ActionInvocation::from_id_and_payload(
            ActionId::ToggleOverview,
            Some(serde_json::json!(null)),
        )
        .unwrap();
        assert_eq!(inv_null, ActionInvocation::ToggleOverview);

        let err = ActionInvocation::from_id_and_payload(
            ActionId::ToggleOverview,
            Some(serde_json::json!({"foo": "bar"})),
        )
        .unwrap_err();
        assert!(err.contains("does not accept input parameters"));
        assert!(err.contains("builtin:toggle_overview"));
    }

    #[test]
    fn from_id_and_payload_parameterized_actions() {
        let inv_ws = ActionInvocation::from_id_and_payload(
            ActionId::FocusWorkspace,
            Some(serde_json::json!({"workspace_id": 7})),
        )
        .unwrap();
        assert_eq!(inv_ws, ActionInvocation::FocusWorkspace(7));

        let inv_win = ActionInvocation::from_id_and_payload(
            ActionId::FocusWindow,
            Some(serde_json::json!({"window_id": 42})),
        )
        .unwrap();
        assert_eq!(inv_win, ActionInvocation::FocusWindow(42));

        let inv_close = ActionInvocation::from_id_and_payload(
            ActionId::CloseWindow,
            Some(serde_json::json!({"window_id": 42})),
        )
        .unwrap();
        assert_eq!(inv_close, ActionInvocation::CloseWindow(42));

        let inv_move = ActionInvocation::from_id_and_payload(
            ActionId::MoveWindowToWorkspace,
            Some(serde_json::json!({"window_id": 42, "workspace_id": 7})),
        )
        .unwrap();
        assert_eq!(
            inv_move,
            ActionInvocation::MoveWindowToWorkspace {
                window_id: 42,
                workspace_id: 7
            }
        );
    }

    #[test]
    fn from_id_and_payload_parameterized_rejects_missing_null_and_malformed() {
        let parameterized = vec![
            ActionId::FocusWorkspace,
            ActionId::FocusWindow,
            ActionId::CloseWindow,
            ActionId::MoveWindowToWorkspace,
        ];

        for id in parameterized {
            assert!(ActionInvocation::from_id_and_payload(id.clone(), None).is_err());
            assert!(
                ActionInvocation::from_id_and_payload(id.clone(), Some(serde_json::json!(null)))
                    .is_err()
            );
        }

        // Missing required field
        let err_missing = ActionInvocation::from_id_and_payload(
            ActionId::FocusWorkspace,
            Some(serde_json::json!({})),
        )
        .unwrap_err();
        assert!(err_missing.contains("workspace_id"));
        assert!(err_missing.contains("builtin:focus_workspace"));

        // Wrong type (string instead of u64)
        let err_type = ActionInvocation::from_id_and_payload(
            ActionId::FocusWindow,
            Some(serde_json::json!({"window_id": "42"})),
        )
        .unwrap_err();
        assert!(err_type.contains("window_id"));

        // Negative number
        let err_neg = ActionInvocation::from_id_and_payload(
            ActionId::CloseWindow,
            Some(serde_json::json!({"window_id": -1})),
        )
        .unwrap_err();
        assert!(err_neg.contains("window_id"));

        // Unknown extra field
        let err_extra = ActionInvocation::from_id_and_payload(
            ActionId::MoveWindowToWorkspace,
            Some(serde_json::json!({"window_id": 1, "workspace_id": 2, "extra": true})),
        )
        .unwrap_err();
        assert!(err_extra.contains("builtin:move_window_to_workspace"));
    }

    #[test]
    fn from_id_and_payload_preserves_extension_payloads() {
        let canonical: CanonicalId = "io.github.alice.world-clock/refresh".parse().unwrap();
        let ext_id = ActionId::extension(canonical.clone());

        // None
        let inv_none = ActionInvocation::from_id_and_payload(ext_id.clone(), None).unwrap();
        assert_eq!(
            inv_none,
            ActionInvocation::Extension {
                id: canonical.clone(),
                payload: None
            }
        );

        // Some(null)
        let inv_null =
            ActionInvocation::from_id_and_payload(ext_id.clone(), Some(serde_json::json!(null)))
                .unwrap();
        assert_eq!(
            inv_null,
            ActionInvocation::Extension {
                id: canonical.clone(),
                payload: Some(serde_json::json!(null))
            }
        );

        // Arbitrary nested JSON
        let nested = serde_json::json!({"city": "Tokyo", "items": [1, 2, 3]});
        let inv_nested =
            ActionInvocation::from_id_and_payload(ext_id.clone(), Some(nested.clone())).unwrap();
        assert_eq!(
            inv_nested,
            ActionInvocation::Extension {
                id: canonical,
                payload: Some(nested)
            }
        );
    }

    #[test]
    fn shortcut_parsing_and_keybinding_defaults() {
        let sc = Shortcut::parse("Super + Space").unwrap();
        assert_eq!(sc.modifiers, vec!["super"]);
        assert_eq!(sc.key, "space");
        assert_eq!(sc.to_spec(), "super+space");

        let mgr = KeybindingManager::with_defaults();
        assert_eq!(mgr.action_for(&sc), Some(ActionId::ToggleOverview));

        let sc_bar = Shortcut::parse("super+b").unwrap();
        assert_eq!(mgr.action_for(&sc_bar), Some(ActionId::ToggleBar));
    }

    #[test]
    fn shortcut_conflict_detection() {
        let mut mgr = KeybindingManager::new();
        let sc = Shortcut::parse("super+b").unwrap();
        assert!(mgr.register(sc.clone(), ActionId::ToggleBar).is_ok());

        // Conflict registration should fail with diagnostic
        let err = mgr.register(sc, ActionId::Quit).unwrap_err();
        assert!(err.contains("conflict"));
    }

    #[test]
    fn shortcut_conflict_query_and_reset_to_defaults() {
        let mut mgr = KeybindingManager::with_defaults();
        let sc = Shortcut::parse("super+space").unwrap();

        let conflict = mgr.find_conflict(&sc, ActionId::Quit);
        assert_eq!(conflict, Some(ActionId::ToggleOverview));

        let _ = mgr.unregister(&sc);
        assert_eq!(mgr.action_for(&sc), None);

        mgr.reset_to_defaults();
        assert_eq!(mgr.action_for(&sc), Some(ActionId::ToggleOverview));
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
