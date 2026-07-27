use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Component, Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewLimits {
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_text_bytes: usize,
    pub max_list_items: usize,
}

impl Default for ViewLimits {
    fn default() -> Self {
        Self {
            max_depth: 32,
            max_nodes: 1_024,
            max_text_bytes: 64 * 1_024,
            max_list_items: 256,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewValidationError(String);

impl fmt::Display for ViewValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for ViewValidationError {}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ViewTree {
    pub root: ViewNode,
}

impl ViewTree {
    pub fn new(root: ViewNode) -> Self {
        Self { root }
    }

    pub fn validate(&self, limits: ViewLimits) -> Result<(), ViewValidationError> {
        let mut state = ValidationState::default();
        validate_node(&self.root, 1, limits, &mut state)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewNode {
    Container(ContainerNode),
    Text(TextNode),
    Icon(IconNode),
    Image(ImageNode),
    Button(ButtonNode),
    IconButton(IconButtonNode),
    Toggle(ToggleNode),
    Slider(SliderNode),
    TextInput(TextInputNode),
    List(ListNode),
    Spacer(SpacerNode),
    Divider,
    Badge(BadgeNode),
    Progress(ProgressNode),
    LoadingIndicator(LoadingIndicatorNode),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContainerDirection {
    Row,
    Column,
    Stack,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticColorToken {
    Primary,
    OnPrimary,
    Secondary,
    Surface,
    SurfaceContainer,
    OnSurface,
    OnSurfaceVariant,
    Outline,
    Error,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ViewStyle {
    pub padding: Option<f32>,
    pub margin: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub corner_radius: Option<f32>,
    pub opacity: Option<f32>,
    pub color: Option<SemanticColorToken>,
    pub background: Option<SemanticColorToken>,
    pub flex_grow: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContainerNode {
    pub direction: ContainerDirection,
    #[serde(default)]
    pub children: Vec<ViewNode>,
    pub style: Option<ViewStyle>,
    pub gap: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TextNode {
    pub content: String,
    pub font_size: Option<f32>,
    pub bold: Option<bool>,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IconNode {
    pub name: String,
    pub size: Option<f32>,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ImageNode {
    pub asset_path: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ButtonNode {
    pub label: String,
    pub event_id: String,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IconButtonNode {
    pub icon_name: String,
    pub event_id: String,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ToggleNode {
    pub value: bool,
    pub event_id: String,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SliderNode {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub event_id: String,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TextInputNode {
    pub placeholder: Option<String>,
    pub value: String,
    pub event_id: String,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ListNode {
    pub items: Vec<ViewNode>,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpacerNode {
    pub size: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BadgeNode {
    pub label: String,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProgressNode {
    pub value: f32,
    pub style: Option<ViewStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LoadingIndicatorNode {
    pub size: Option<f32>,
    pub color: Option<SemanticColorToken>,
    pub style: Option<ViewStyle>,
}

#[derive(Default)]
struct ValidationState {
    nodes: usize,
    text_bytes: usize,
}

fn validate_node(
    node: &ViewNode,
    depth: usize,
    limits: ViewLimits,
    state: &mut ValidationState,
) -> Result<(), ViewValidationError> {
    state.nodes += 1;
    if depth > limits.max_depth {
        return invalid("view tree exceeds the maximum depth");
    }
    if state.nodes > limits.max_nodes {
        return invalid("view tree exceeds the maximum node count");
    }

    let mut validate_text = |value: &str| {
        state.text_bytes += value.len();
        if state.text_bytes > limits.max_text_bytes {
            invalid("view tree exceeds the maximum text size")
        } else {
            Ok(())
        }
    };

    match node {
        ViewNode::Container(container) => {
            validate_style(container.style.as_ref())?;
            validate_nonnegative("container gap", container.gap)?;
            for child in &container.children {
                validate_node(child, depth + 1, limits, state)?;
            }
        }
        ViewNode::Text(text) => {
            validate_text(&text.content)?;
            validate_positive("font size", text.font_size)?;
            validate_style(text.style.as_ref())?;
        }
        ViewNode::Icon(icon) => {
            validate_icon_name(&icon.name)?;
            validate_positive("icon size", icon.size)?;
            validate_style(icon.style.as_ref())?;
        }
        ViewNode::Image(image) => {
            validate_asset_path(&image.asset_path)?;
            validate_positive("image width", image.width)?;
            validate_positive("image height", image.height)?;
            validate_style(image.style.as_ref())?;
        }
        ViewNode::Button(button) => {
            validate_text(&button.label)?;
            validate_event_id(&button.event_id)?;
            validate_style(button.style.as_ref())?;
        }
        ViewNode::IconButton(button) => {
            validate_icon_name(&button.icon_name)?;
            validate_event_id(&button.event_id)?;
            validate_style(button.style.as_ref())?;
        }
        ViewNode::Toggle(toggle) => {
            validate_event_id(&toggle.event_id)?;
            validate_style(toggle.style.as_ref())?;
        }
        ViewNode::Slider(slider) => {
            if !slider.min.is_finite()
                || !slider.max.is_finite()
                || !slider.value.is_finite()
                || slider.min >= slider.max
                || !(slider.min..=slider.max).contains(&slider.value)
            {
                return invalid("slider range or value is invalid");
            }
            validate_event_id(&slider.event_id)?;
            validate_style(slider.style.as_ref())?;
        }
        ViewNode::TextInput(input) => {
            if let Some(placeholder) = &input.placeholder {
                validate_text(placeholder)?;
            }
            validate_text(&input.value)?;
            validate_event_id(&input.event_id)?;
            validate_style(input.style.as_ref())?;
        }
        ViewNode::List(list) => {
            if list.items.len() > limits.max_list_items {
                return invalid("list exceeds the maximum item count");
            }
            validate_style(list.style.as_ref())?;
            for item in &list.items {
                validate_node(item, depth + 1, limits, state)?;
            }
        }
        ViewNode::Spacer(spacer) => validate_nonnegative("spacer size", spacer.size)?,
        ViewNode::Divider => {}
        ViewNode::Badge(badge) => {
            validate_text(&badge.label)?;
            validate_style(badge.style.as_ref())?;
        }
        ViewNode::Progress(progress) => {
            if !progress.value.is_finite() || !(0.0..=1.0).contains(&progress.value) {
                return invalid("progress value must be between zero and one");
            }
            validate_style(progress.style.as_ref())?;
        }
        ViewNode::LoadingIndicator(indicator) => {
            if let Some(size) = indicator.size
                && (!size.is_finite() || size <= 0.0)
            {
                return invalid("loading indicator size must be positive");
            }
            validate_style(indicator.style.as_ref())?;
        }
    }
    Ok(())
}

fn validate_style(style: Option<&ViewStyle>) -> Result<(), ViewValidationError> {
    let Some(style) = style else {
        return Ok(());
    };
    validate_nonnegative("padding", style.padding)?;
    validate_nonnegative("margin", style.margin)?;
    validate_nonnegative("width", style.width)?;
    validate_nonnegative("height", style.height)?;
    validate_nonnegative("corner radius", style.corner_radius)?;
    validate_nonnegative("flex grow", style.flex_grow)?;
    if style
        .opacity
        .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return invalid("opacity must be between zero and one");
    }
    Ok(())
}

fn validate_nonnegative(field: &str, value: Option<f32>) -> Result<(), ViewValidationError> {
    if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
        return invalid(format!("{field} must be finite and non-negative"));
    }
    Ok(())
}

fn validate_positive(field: &str, value: Option<f32>) -> Result<(), ViewValidationError> {
    if value.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        return invalid(format!("{field} must be finite and positive"));
    }
    Ok(())
}

fn validate_event_id(value: &str) -> Result<(), ViewValidationError> {
    if value.is_empty()
        || value.len() > 128
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')))
    {
        return invalid("event ID has an invalid format");
    }
    Ok(())
}

fn validate_icon_name(value: &str) -> Result<(), ViewValidationError> {
    if value.is_empty()
        || value.len() > 64
        || value.bytes().any(|byte| {
            !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
        })
    {
        return invalid("icon name has an invalid format");
    }
    Ok(())
}

fn validate_asset_path(value: &str) -> Result<(), ViewValidationError> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return invalid("image asset path must be a safe relative path");
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ViewValidationError> {
    Err(ViewValidationError(message.into()))
}
