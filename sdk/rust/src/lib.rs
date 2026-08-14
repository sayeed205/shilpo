//! # Shilpo Extension Rust SDK (`shilpo-ext-sdk`)
//!
//! Official Rust SDK for developing sandboxed WebAssembly extensions for the
//! [Shilpo](https://github.com/sayeed205/shilpo) Linux desktop environment.
//!
//! ## Overview
//!
//! `shilpo-ext-sdk` provides an idiomatic, high-level Rust interface for authoring
//! extensions targeting the canonical `shilpo:extension@0.1.0` WebAssembly Component Model
//! interface:
//!
//! - **Declarative ViewTree Builders**: Fluent builders for constructing UI trees across all 15
//!   canonical node types with zero manual index management.
//! - **`view!` Macro**: Declarative JSX-like macro supporting nested containers, conditionals,
//!   loops, and expressions.
//! - **Typed Lifecycle Adapter**: The [`Extension`] trait and [`export_extension!`] macro for
//!   activation, event dispatch, and UI rendering.
//! - **State Helper**: High-level [`State`] API for namespaced key-value storage and reactive watches.
//! - **DataValue Conversions**: Ergonomic traits and constructors for WIT `data-value` scalar types.
//!
//! ## Minimal Extension Example
//!
//! ```rust,no_run
//! use shilpo_ext_sdk::prelude::*;
//!
//! #[derive(Default)]
//! struct HelloExtension;
//!
//! impl Extension for HelloExtension {
//!     fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
//!         if contribution_id == "widget" {
//!             Ok(Some(view! {
//!                 row {
//!                     icon("hand-wave").size(16.0),
//!                     text("Hello from Rust!"),
//!                     button("Click Me", "on-click"),
//!                 }
//!             }))
//!         } else {
//!             Ok(None)
//!         }
//!     }
//! }
//!
//! export_extension!(HelloExtension);
//! ```
//!
//! ## Compile-Fail Validation
//!
//! Invalid builder methods or non-existent view elements are rejected at compile time:
//!
//! ```compile_fail
//! use shilpo_ext_sdk::prelude::*;
//! // Unknown builder method fails at compile time:
//! let _ = text("test").non_existent_method(123);
//! ```
//!
//! ```compile_fail
//! use shilpo_ext_sdk::prelude::*;
//! // Passing non-view-node type to child fails at compile time:
//! let _ = row().child(12345);
//! ```

#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod bindings;
pub mod builder;
pub mod data;
pub mod extension;
pub mod macros;
pub mod prelude;
pub mod state;

pub use bindings as raw;
pub use prelude::*;
