//! The Shilpo Extension SDK prelude.
//!
//! Re-exports commonly used types, traits, builders, and macros for developing
//! sandboxed WebAssembly desktop extensions.

pub use crate::bindings::shilpo::extension::{
    events::{ExtensionEvent, InputEvent},
    state::{StateMutation, StateSnapshot, WatchRegistration},
    types::{Activation, DataValue, DeactivateReason, Error, ErrorKind, SecretRef},
    view::{
        Alignment, ContainerDirection, Justification, Overflow, SemanticColorToken, ViewNode,
        ViewStyle, ViewTree,
    },
};

pub use crate::builder::{
    BadgeBuilder, ButtonBuilder, ContainerBuilder, IconBuilder, IconButtonBuilder, ImageBuilder,
    IntoViewNode, ListBuilder, LoadingIndicatorBuilder, ProgressBuilder, SliderBuilder,
    SpacerBuilder, StyleBuilder, TextBuilder, TextInputBuilder, ToggleBuilder, badge,
    build_view_tree, button, column, container, divider, grid, icon, icon_button, image, list,
    loading_indicator, progress, row, slider, spacer, stack, style, text, text_input, toggle,
};

pub use crate::data::DataValueExt;
pub use crate::extension::Extension;
pub use crate::state::State;
pub use crate::{export_extension, view};
