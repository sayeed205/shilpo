//! `bar::cards` — two-channel card coordinator for bar widget supplementary surfaces.
//!
//! This module hides hover timing, two-channel state, placement, focus, and
//! layer-shell lifecycle behind a narrow shell interface.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │ bar::cards                                                   │
//! │                                                              │
//! │  model.rs     – Pure two-channel state machine + reducer     │
//! │  placement.rs – Pure deterministic placement engine          │
//! │  provider.rs  – CardProvider trait (widget contract)         │
//! │  band.rs      – CardBandView GPUI Render entity              │
//! │  adapter.rs   – CardCoordinator (effect interpreter / GPUI)  │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Shell interface
//!
//! - [`adapter::CardCoordinator::register_provider`] / [`adapter::CardCoordinator::remove_provider`]
//! - [`adapter::CardCoordinator::dispatch`] — route semantic `CardRequest` events
//! - [`adapter::CardCoordinator::source_state`] — widget rendering state token
//! - [`adapter::CardCoordinator::holds_bar_visibility`] — infrastructure hold signal
//!
pub(crate) mod adapter;
pub(crate) mod band;
pub(crate) mod model;
pub(crate) mod placement;
pub(crate) mod provider;
