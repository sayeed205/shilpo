pub mod adapters;
pub mod client;
pub mod daemon;
pub mod dbus;
pub mod persistence;
pub mod portal;
pub mod state;

pub use client::ThemeClient;
pub use persistence::{read_state_snapshot, state_file_path, write_state_snapshot};
pub use state::{ColorSource, SideEffect, ThemeCommand, ThemeMode, ThemeState, reduce};
