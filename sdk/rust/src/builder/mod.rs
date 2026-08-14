//! Declarative ViewTree builders for constructing typed UI component trees.

pub mod nodes;
pub mod style;
pub mod tree;

pub use nodes::{
    BadgeBuilder, ButtonBuilder, ContainerBuilder, IconBuilder, IconButtonBuilder, ImageBuilder,
    IntoViewNode, ListBuilder, LoadingIndicatorBuilder, NodeSpec, ProgressBuilder, SliderBuilder,
    SpacerBuilder, TextBuilder, TextInputBuilder, ToggleBuilder, badge, button, column, container,
    divider, grid, icon, icon_button, image, list, loading_indicator, progress, row, slider,
    spacer, stack, text, text_input, toggle,
};
pub use style::{StyleBuilder, style};
pub use tree::build_view_tree;
