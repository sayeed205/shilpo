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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutOrigin {
    User,
    BuiltinDefault,
    ExtensionDefault,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedShortcut {
    pub action_id: ActionId,
    pub shortcut: Shortcut,
    pub origin: ShortcutOrigin,
    pub extension_id: Option<shilpo_ext_api::ExtensionId>,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeybindingDiagnostic {
    pub action_id: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeybindingReconciliationReport {
    pub resolved: Vec<ResolvedShortcut>,
    pub diagnostics: Vec<KeybindingDiagnostic>,
}

/// Representation of a parsed key combination shortcut (e.g. "Super+Space").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shortcut {
    pub modifiers: Vec<String>,
    pub key: String,
}

impl Shortcut {
    pub fn parse(spec: &str) -> Option<Self> {
        let canonical = shilpo_ext_api::manifest::validate_shortcut_spec(spec).ok()?;
        let parts: Vec<&str> = canonical.split('+').collect();
        let key = parts.last()?.to_string();
        let modifiers = parts[..parts.len() - 1]
            .iter()
            .map(|s| s.to_string())
            .collect();
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
#[derive(Debug, Clone, Default)]
pub struct KeybindingManager {
    bindings_by_shortcut: std::collections::HashMap<Shortcut, ActionId>,
    resolved: Vec<ResolvedShortcut>,
}

impl KeybindingManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_defaults() -> Self {
        let mut mgr = Self::new();
        mgr.reconcile(&[], &ActionRegistry::default().all(), &[]);
        mgr
    }

    pub fn reconcile(
        &mut self,
        user_bindings: &[crate::config::KeybindingConfig],
        builtin_actions: &[ActionDescriptor],
        extension_shortcuts: &[shilpo_ext_runtime::worker::protocol::ContributionDescriptor],
    ) -> KeybindingReconciliationReport {
        let mut report = KeybindingReconciliationReport::default();
        let previous_resolved = self.resolved.clone();
        let previous_bindings = self.bindings_by_shortcut.clone();
        let mut invalid_user_binding = false;
        let mut candidates: Vec<(
            u8,
            ActionId,
            Shortcut,
            ShortcutOrigin,
            String,
            Option<shilpo_ext_api::ExtensionId>,
        )> = Vec::new();
        let mut disabled_actions: std::collections::HashSet<ActionId> =
            std::collections::HashSet::new();

        // 1. Explicit user bindings (Precedence: User)
        for kb in user_bindings {
            let Ok(action_id) = kb.action.parse::<ActionId>() else {
                invalid_user_binding = true;
                report.diagnostics.push(KeybindingDiagnostic {
                    action_id: kb.action.clone(),
                    message: format!("user keybinding action ID '{}' is malformed", kb.action),
                });
                continue;
            };

            let is_known = builtin_actions.iter().any(|b| b.id == action_id)
                || extension_shortcuts.iter().any(|s| {
                    s.action
                        .as_ref()
                        .is_some_and(|target| ActionId::extension(target.clone()) == action_id)
                });
            if !is_known {
                report.diagnostics.push(KeybindingDiagnostic {
                    action_id: action_id.to_string(),
                    message: format!(
                        "user keybinding action '{action_id}' is currently unavailable"
                    ),
                });
                continue;
            }

            if !kb.enabled {
                disabled_actions.insert(action_id);
            } else if let Some(spec) = &kb.shortcut
                && let Some(shortcut) = Shortcut::parse(spec)
            {
                let name = builtin_actions
                    .iter()
                    .find(|b| b.id == action_id)
                    .map(|b| b.label.clone())
                    .or_else(|| {
                        extension_shortcuts
                            .iter()
                            .find(|s| {
                                s.action.as_ref().is_some_and(|target| {
                                    ActionId::extension(target.clone()) == action_id
                                })
                            })
                            .map(|s| s.name.clone())
                    })
                    .unwrap_or_else(|| action_id.name().to_string());

                let ext_id = action_id.extension_id().map(|c| c.extension_id);
                candidates.push((0, action_id, shortcut, ShortcutOrigin::User, name, ext_id));
            } else if kb.enabled {
                invalid_user_binding = true;
            }
        }

        // Built-in defaults mapping
        let builtin_defaults: &[(&str, ActionId)] = &[
            ("Super+Space", ActionId::ToggleOverview),
            ("Super+B", ActionId::ToggleBar),
            ("Super+Shift+R", ActionId::ReloadConfig),
            ("Super+Shift+Q", ActionId::Quit),
        ];

        // 2. Built-in defaults (Precedence: BuiltinDefault)
        for (spec, action_id) in builtin_defaults {
            if disabled_actions.contains(action_id)
                || candidates.iter().any(|(_, id, ..)| id == action_id)
            {
                continue;
            }
            if let Some(desc) = builtin_actions.iter().find(|b| b.id == *action_id)
                && let Some(shortcut) = Shortcut::parse(spec)
            {
                candidates.push((
                    1,
                    action_id.clone(),
                    shortcut,
                    ShortcutOrigin::BuiltinDefault,
                    desc.label.clone(),
                    None,
                ));
            }
        }

        // 3. Extension-recommended defaults (Precedence: ExtensionDefault)
        for desc in extension_shortcuts {
            if desc.surface != shilpo_ext_runtime::worker::protocol::ContributionSurface::Shortcut {
                continue;
            }
            let Some(target_canonical) = &desc.action else {
                continue;
            };
            let action_id = ActionId::extension(target_canonical.clone());
            if disabled_actions.contains(&action_id)
                || candidates.iter().any(|(_, id, ..)| id == &action_id)
            {
                continue;
            }
            if let Some(spec) = &desc.default_binding
                && let Some(shortcut) = Shortcut::parse(spec)
            {
                candidates.push((
                    2,
                    action_id,
                    shortcut,
                    ShortcutOrigin::ExtensionDefault,
                    desc.name.clone(),
                    Some(target_canonical.extension_id.clone()),
                ));
            }
        }

        // 4. Resolve conflicts deterministically
        let mut used_shortcuts: std::collections::HashMap<Shortcut, ActionId> =
            std::collections::HashMap::new();
        let mut resolved_list = Vec::new();

        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        for (_, action_id, shortcut, origin, name, ext_id) in candidates {
            if let Some(existing_action_id) = used_shortcuts.get(&shortcut) {
                report.diagnostics.push(KeybindingDiagnostic {
                    action_id: action_id.to_string(),
                    message: format!(
                        "shortcut '{}' for action '{action_id}' ({origin:?}) collides with higher-precedence binding for action '{existing_action_id}'",
                        shortcut.to_spec()
                    ),
                });
            } else {
                used_shortcuts.insert(shortcut.clone(), action_id.clone());
                resolved_list.push(ResolvedShortcut {
                    action_id,
                    shortcut,
                    origin,
                    extension_id: ext_id,
                    name,
                });
            }
        }

        if invalid_user_binding {
            self.bindings_by_shortcut = previous_bindings;
            self.resolved = previous_resolved.clone();
            report.resolved = previous_resolved;
        } else {
            self.bindings_by_shortcut = used_shortcuts;
            self.resolved = resolved_list.clone();
            report.resolved = resolved_list;
        }
        report
    }

    pub fn resolved_shortcuts(&self) -> &[ResolvedShortcut] {
        &self.resolved
    }

    pub fn action_for(&self, shortcut: &Shortcut) -> Option<ActionId> {
        self.bindings_by_shortcut.get(shortcut).cloned()
    }

    pub fn shortcut_for(&self, action: &ActionId) -> Option<Shortcut> {
        self.resolved
            .iter()
            .find(|r| r.action_id == *action)
            .map(|r| r.shortcut.clone())
    }

    pub fn keybinding_descriptors(&self) -> Vec<(String, String)> {
        self.resolved
            .iter()
            .map(|r| (r.shortcut.to_spec(), r.name.clone()))
            .collect()
    }

    pub fn reset_to_defaults(&mut self) {
        *self = Self::with_defaults();
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
        let sc = Shortcut::parse("Super+Space").unwrap();
        assert_eq!(sc.modifiers, vec!["Super"]);
        assert_eq!(sc.key, "Space");
        assert_eq!(sc.to_spec(), "Super+Space");

        let mgr = KeybindingManager::with_defaults();
        assert_eq!(mgr.action_for(&sc), Some(ActionId::ToggleOverview));

        let sc_bar = Shortcut::parse("Super+B").unwrap();
        assert_eq!(mgr.action_for(&sc_bar), Some(ActionId::ToggleBar));
    }

    #[test]
    fn shortcut_conflict_detection_and_precedence() {
        let mut mgr = KeybindingManager::new();
        let user_bindings = vec![crate::config::KeybindingConfig {
            action: "builtin:toggle_bar".into(),
            shortcut: Some("Super+Space".into()),
            enabled: true,
        }];
        let builtin_actions = ActionRegistry::default().all();

        let report = mgr.reconcile(&user_bindings, &builtin_actions, &[]);
        let sc_space = Shortcut::parse("Super+Space").unwrap();

        // User binding for toggle_bar overrides built-in default for toggle_overview
        assert_eq!(mgr.action_for(&sc_space), Some(ActionId::ToggleBar));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("collides"))
        );
    }

    #[test]
    fn user_extension_binding_beats_builtin_and_extension_defaults() {
        let extension_id = shilpo_ext_api::ExtensionId::new("io.example.shortcuts").unwrap();
        let contribution = shilpo_ext_api::ContributionId::new("open").unwrap();
        let canonical = shilpo_ext_api::CanonicalId::new(extension_id, contribution);
        let action_id = ActionId::extension(canonical.clone());
        let descriptor = shilpo_ext_runtime::worker::protocol::ContributionDescriptor {
            id: canonical.clone(),
            extension_name: "Example".into(),
            name: "Open panel".into(),
            surface: shilpo_ext_runtime::worker::protocol::ContributionSurface::Shortcut,
            runtime_kind: shilpo_ext_runtime::worker::protocol::ExtensionRuntimeKind::Wasm,
            settings_schema: None,
            default_size: None,
            minimum_size: None,
            bar_widget: None,
            action: Some(canonical),
            default_binding: Some("Super+Space".into()),
            wallpaper_modes: None,
            wallpaper_targets: None,
        };
        let mut manager = KeybindingManager::new();
        let user = vec![crate::config::KeybindingConfig {
            action: action_id.to_string(),
            shortcut: Some("Super+Space".into()),
            enabled: true,
        }];
        let report = manager.reconcile(&user, &ActionRegistry::default().all(), &[descriptor]);
        assert_eq!(
            manager.action_for(&Shortcut::parse("Super+Space").unwrap()),
            Some(action_id)
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("collides"))
        );
    }

    #[test]
    fn shortcut_reset_to_defaults() {
        let mut mgr = KeybindingManager::new();
        let user_bindings = vec![crate::config::KeybindingConfig {
            action: "builtin:toggle_overview".into(),
            shortcut: Some("Super+A".into()),
            enabled: true,
        }];
        let builtin_actions = ActionRegistry::default().all();
        mgr.reconcile(&user_bindings, &builtin_actions, &[]);

        let sc_a = Shortcut::parse("Super+A").unwrap();
        assert_eq!(mgr.action_for(&sc_a), Some(ActionId::ToggleOverview));

        mgr.reset_to_defaults();
        assert_eq!(mgr.action_for(&sc_a), None);
        let sc_space = Shortcut::parse("Super+Space").unwrap();
        assert_eq!(mgr.action_for(&sc_space), Some(ActionId::ToggleOverview));
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
