//! Node builders for all 15 canonical ViewTree elements.

use crate::bindings::shilpo::extension::view::{
    Alignment, ContainerDirection, Justification, SemanticColorToken, ViewStyle, ViewTree,
};
use crate::builder::tree::build_view_tree;

/// Internal representation of a tree node before flattening.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeSpec {
    /// Container node (row, column, stack, grid).
    Container(ContainerBuilder),
    /// Text label node.
    Text(TextBuilder),
    /// SVG icon node.
    Icon(IconBuilder),
    /// Virtual image asset node.
    Image(ImageBuilder),
    /// Push button node.
    Button(ButtonBuilder),
    /// Icon-only button node.
    IconButton(IconButtonBuilder),
    /// Switch toggle node.
    Toggle(ToggleBuilder),
    /// Range slider node.
    Slider(SliderBuilder),
    /// Single-line text input node.
    TextInput(TextInputBuilder),
    /// Linear list container node.
    List(ListBuilder),
    /// Spacer node.
    Spacer(SpacerBuilder),
    /// Divider line node.
    Divider,
    /// Status badge indicator node.
    Badge(BadgeBuilder),
    /// Progress bar node.
    Progress(ProgressBuilder),
    /// Indeterminate loading indicator node.
    LoadingIndicator(LoadingIndicatorBuilder),
}

/// Trait implemented by all builder types that can be converted into a [`NodeSpec`].
pub trait IntoViewNode {
    /// Converts this value into a node specification.
    fn into_node_spec(self) -> NodeSpec;
}

impl IntoViewNode for NodeSpec {
    fn into_node_spec(self) -> NodeSpec {
        self
    }
}

// ---------------------------------------------------------------------------
// Container Builders
// ---------------------------------------------------------------------------

/// Builder for generic and directional container nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerBuilder {
    pub(crate) direction: ContainerDirection,
    pub(crate) children: Vec<NodeSpec>,
    pub(crate) style: Option<ViewStyle>,
    pub(crate) gap: Option<f32>,
    pub(crate) align_items: Option<Alignment>,
    pub(crate) justify_content: Option<Justification>,
    pub(crate) wrap: bool,
    pub(crate) event_id: Option<String>,
}

impl ContainerBuilder {
    /// Creates a new container with the specified direction.
    pub fn new(direction: ContainerDirection) -> Self {
        Self {
            direction,
            children: Vec::new(),
            style: None,
            gap: None,
            align_items: None,
            justify_content: None,
            wrap: false,
            event_id: None,
        }
    }

    /// Sets the layout direction of the container.
    pub fn direction(mut self, direction: ContainerDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Appends a child node to this container.
    pub fn child(mut self, child: impl IntoViewNode) -> Self {
        self.children.push(child.into_node_spec());
        self
    }

    /// Appends an optional child node if present.
    pub fn child_opt(mut self, child: Option<impl IntoViewNode>) -> Self {
        if let Some(c) = child {
            self.children.push(c.into_node_spec());
        }
        self
    }

    /// Appends multiple child nodes to this container.
    pub fn children<I, N>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = N>,
        N: IntoViewNode,
    {
        self.children
            .extend(children.into_iter().map(IntoViewNode::into_node_spec));
        self
    }

    /// Sets the visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Sets the gap between child items in pixels.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = Some(gap);
        self
    }

    /// Sets cross-axis alignment.
    pub fn align_items(mut self, align: Alignment) -> Self {
        self.align_items = Some(align);
        self
    }

    /// Sets main-axis distribution.
    pub fn justify_content(mut self, justify: Justification) -> Self {
        self.justify_content = Some(justify);
        self
    }

    /// Sets whether children wrap to multiple lines.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Sets an interactive event ID for clicking the container background.
    pub fn event_id(mut self, id: impl Into<String>) -> Self {
        self.event_id = Some(id.into());
        self
    }

    /// Flattens this container and all descendants into a canonical [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for ContainerBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Container(self)
    }
}

/// Creates a container with default column direction.
pub fn container() -> ContainerBuilder {
    ContainerBuilder::new(ContainerDirection::Column)
}

/// Creates a horizontal row container.
pub fn row() -> ContainerBuilder {
    ContainerBuilder::new(ContainerDirection::Row)
}

/// Creates a vertical column container.
pub fn column() -> ContainerBuilder {
    ContainerBuilder::new(ContainerDirection::Column)
}

/// Creates an overlapping stack container.
pub fn stack() -> ContainerBuilder {
    ContainerBuilder::new(ContainerDirection::Stack)
}

/// Creates a grid container with fixed column count.
pub fn grid(columns: u16) -> ContainerBuilder {
    ContainerBuilder::new(ContainerDirection::Grid(columns))
}

// ---------------------------------------------------------------------------
// Text Builder
// ---------------------------------------------------------------------------

/// Builder for text label nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBuilder {
    pub(crate) content: String,
    pub(crate) font_size: Option<f32>,
    pub(crate) bold: Option<bool>,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates a text node with the given content.
pub fn text(content: impl Into<String>) -> TextBuilder {
    TextBuilder {
        content: content.into(),
        font_size: None,
        bold: None,
        style: None,
    }
}

impl TextBuilder {
    /// Sets font size in points.
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }

    /// Sets bold weight.
    pub fn bold(mut self, bold: bool) -> Self {
        self.bold = Some(bold);
        self
    }

    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for TextBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Text(self)
    }
}

// ---------------------------------------------------------------------------
// Icon Builder
// ---------------------------------------------------------------------------

/// Builder for SVG icon nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct IconBuilder {
    pub(crate) name: String,
    pub(crate) size: Option<f32>,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates an icon node with the given canonical icon name.
pub fn icon(name: impl Into<String>) -> IconBuilder {
    IconBuilder {
        name: name.into(),
        size: None,
        style: None,
    }
}

impl IconBuilder {
    /// Sets icon render size (width and height) in pixels.
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for IconBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Icon(self)
    }
}

// ---------------------------------------------------------------------------
// Image Builder
// ---------------------------------------------------------------------------

/// Builder for extension image nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageBuilder {
    pub(crate) asset_path: String,
    pub(crate) width: Option<f32>,
    pub(crate) height: Option<f32>,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates an image node referencing a relative virtual asset path.
pub fn image(asset_path: impl Into<String>) -> ImageBuilder {
    ImageBuilder {
        asset_path: asset_path.into(),
        width: None,
        height: None,
        style: None,
    }
}

impl ImageBuilder {
    /// Sets explicit image width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Sets explicit image height.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Sets both width and height simultaneously.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for ImageBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Image(self)
    }
}

// ---------------------------------------------------------------------------
// Button Builder
// ---------------------------------------------------------------------------

/// Builder for push button nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonBuilder {
    pub(crate) label: String,
    pub(crate) event_id: String,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates an interactive button with label text and click event ID.
pub fn button(label: impl Into<String>, event_id: impl Into<String>) -> ButtonBuilder {
    ButtonBuilder {
        label: label.into(),
        event_id: event_id.into(),
        style: None,
    }
}

impl ButtonBuilder {
    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for ButtonBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Button(self)
    }
}

// ---------------------------------------------------------------------------
// IconButton Builder
// ---------------------------------------------------------------------------

/// Builder for icon-only button nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct IconButtonBuilder {
    pub(crate) icon_name: String,
    pub(crate) event_id: String,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates an interactive icon button with icon name and click event ID.
pub fn icon_button(icon_name: impl Into<String>, event_id: impl Into<String>) -> IconButtonBuilder {
    IconButtonBuilder {
        icon_name: icon_name.into(),
        event_id: event_id.into(),
        style: None,
    }
}

impl IconButtonBuilder {
    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for IconButtonBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::IconButton(self)
    }
}

// ---------------------------------------------------------------------------
// Toggle Builder
// ---------------------------------------------------------------------------

/// Builder for switch toggle nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct ToggleBuilder {
    pub(crate) value: bool,
    pub(crate) event_id: String,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates a toggle switch with initial value and change event ID.
pub fn toggle(value: bool, event_id: impl Into<String>) -> ToggleBuilder {
    ToggleBuilder {
        value,
        event_id: event_id.into(),
        style: None,
    }
}

impl ToggleBuilder {
    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for ToggleBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Toggle(self)
    }
}

// ---------------------------------------------------------------------------
// Slider Builder
// ---------------------------------------------------------------------------

/// Builder for range slider nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct SliderBuilder {
    pub(crate) value: f32,
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) event_id: String,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates a range slider with current value, range bounds, and change event ID.
pub fn slider(value: f32, min: f32, max: f32, event_id: impl Into<String>) -> SliderBuilder {
    SliderBuilder {
        value,
        min,
        max,
        event_id: event_id.into(),
        style: None,
    }
}

impl SliderBuilder {
    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for SliderBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Slider(self)
    }
}

// ---------------------------------------------------------------------------
// TextInput Builder
// ---------------------------------------------------------------------------

/// Builder for single-line text input fields.
#[derive(Clone, Debug, PartialEq)]
pub struct TextInputBuilder {
    pub(crate) placeholder: Option<String>,
    pub(crate) value: String,
    pub(crate) event_id: String,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates a text input node with current value and input change event ID.
pub fn text_input(value: impl Into<String>, event_id: impl Into<String>) -> TextInputBuilder {
    TextInputBuilder {
        placeholder: None,
        value: value.into(),
        event_id: event_id.into(),
        style: None,
    }
}

impl TextInputBuilder {
    /// Sets placeholder hint text.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for TextInputBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::TextInput(self)
    }
}

// ---------------------------------------------------------------------------
// List Builder
// ---------------------------------------------------------------------------

/// Builder for linear list containers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListBuilder {
    pub(crate) items: Vec<NodeSpec>,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates a new empty list container.
pub fn list() -> ListBuilder {
    ListBuilder::default()
}

impl ListBuilder {
    /// Appends an item to the list.
    pub fn item(mut self, item: impl IntoViewNode) -> Self {
        self.items.push(item.into_node_spec());
        self
    }

    /// Appends an optional item if present.
    pub fn item_opt(mut self, item: Option<impl IntoViewNode>) -> Self {
        if let Some(i) = item {
            self.items.push(i.into_node_spec());
        }
        self
    }

    /// Appends multiple items to the list.
    pub fn items<I, N>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = N>,
        N: IntoViewNode,
    {
        self.items
            .extend(items.into_iter().map(IntoViewNode::into_node_spec));
        self
    }

    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this list into a canonical [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for ListBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::List(self)
    }
}

// ---------------------------------------------------------------------------
// Spacer Builder
// ---------------------------------------------------------------------------

/// Builder for flexible or fixed spacing elements.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpacerBuilder {
    pub(crate) size: Option<f32>,
}

/// Creates a flexible or fixed spacer node.
pub fn spacer() -> SpacerBuilder {
    SpacerBuilder::default()
}

impl SpacerBuilder {
    /// Sets fixed spacer dimension in pixels.
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for SpacerBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Spacer(self)
    }
}

// ---------------------------------------------------------------------------
// Divider
// ---------------------------------------------------------------------------

/// Creates a visual divider line node.
pub fn divider() -> NodeSpec {
    NodeSpec::Divider
}

// ---------------------------------------------------------------------------
// Badge Builder
// ---------------------------------------------------------------------------

/// Builder for status badge indicator nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct BadgeBuilder {
    pub(crate) label: String,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates a badge node with label text.
pub fn badge(label: impl Into<String>) -> BadgeBuilder {
    BadgeBuilder {
        label: label.into(),
        style: None,
    }
}

impl BadgeBuilder {
    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for BadgeBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Badge(self)
    }
}

// ---------------------------------------------------------------------------
// Progress Builder
// ---------------------------------------------------------------------------

/// Builder for deterministic progress bars.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressBuilder {
    pub(crate) value: f32,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates a progress bar with normalized progress value in range `0.0..=1.0`.
pub fn progress(value: f32) -> ProgressBuilder {
    ProgressBuilder { value, style: None }
}

impl ProgressBuilder {
    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for ProgressBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::Progress(self)
    }
}

// ---------------------------------------------------------------------------
// LoadingIndicator Builder
// ---------------------------------------------------------------------------

/// Builder for animated activity indicators.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadingIndicatorBuilder {
    pub(crate) size: Option<f32>,
    pub(crate) color: Option<SemanticColorToken>,
    pub(crate) style: Option<ViewStyle>,
}

/// Creates an indeterminate loading indicator.
pub fn loading_indicator() -> LoadingIndicatorBuilder {
    LoadingIndicatorBuilder::default()
}

impl LoadingIndicatorBuilder {
    /// Sets indicator diameter in pixels.
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Sets indicator foreground semantic color.
    pub fn color(mut self, color: SemanticColorToken) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets visual style.
    pub fn style(mut self, style: impl Into<ViewStyle>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Flattens this node into a single-node [`ViewTree`].
    pub fn build(self) -> ViewTree {
        build_view_tree(self)
    }
}

impl IntoViewNode for LoadingIndicatorBuilder {
    fn into_node_spec(self) -> NodeSpec {
        NodeSpec::LoadingIndicator(self)
    }
}
