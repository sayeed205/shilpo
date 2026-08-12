//! Public org.shilpo.Shell D-Bus interface, wire types, server, and client proxy.

pub mod client;
pub mod server;
#[cfg(test)]
mod tests;
pub mod types;

pub use client::ShellProxy;
pub use server::{ShellCommand, ShellDbusService};
pub use types::{CommandResult, ShellStatus, ShellTelemetry};
