//! Low-level WebAssembly Interface Type (WIT) bindings.
//!
//! Generated bindings from the canonical `shilpo:extension@0.1.0` WIT contract.

#[allow(clippy::too_many_arguments, missing_docs)]
pub mod generated {
    wit_bindgen::generate!({
        path: "wit",
        world: "extension",
        additional_derives: [PartialEq],
        pub_export_macro: true,
    });
}

pub use generated::export;
pub use generated::shilpo::extension::{
    actions, clipboard, events, filesystem, http, location, notifications, secrets, state, theme,
    types, view, wallpaper,
};
pub use generated::*;
