pub mod actions;
pub mod backend;
pub mod inhibits;
pub mod service;
pub mod state;
pub mod types;

#[cfg(test)]
mod tests;

pub use actions::{
    ActionExecutionOutcome, IdleActionSink, MockIdleActionSink, SystemIdleActionSink,
};
pub use backend::{IdleBackendEvent, IdleNotifierBackend, MockIdleNotifier, WaylandIdleNotifier};
pub use inhibits::{LogindInhibitHolder, ScreenSaverServer};
pub use service::IdleService;
pub use state::IdleDomainState;
pub use types::{
    ActiveGraceInfo, CommandId, CommandResolver, CommandTicket, IdleAction, IdleBehaviorConfig,
    IdleCommand, IdleCommandOutcome, IdlePort, IdleRejectionReason, IdleSnapshot, InhibitSource,
};
