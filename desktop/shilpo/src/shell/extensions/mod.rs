//! Shell-owned extension lifecycle, background worker engine, and coordinator.
//!
//! Surface code talks to `ExtensionCoordinator` instead of the WASM runtime or host engine.
//! UI views read prevalidated, immutable `ExtensionSnapshot`s without calling Wasmtime
//! or scanning package filesystems during rendering.

pub mod coordinator;
pub mod supervisor;
pub mod watcher;

pub use coordinator::ExtensionCoordinator;
pub use shilpo_ext_runtime::{
    ContributionDescriptor, ContributionInstance, ContributionSurface, ExtensionChanges,
    ExtensionCommand, ExtensionGeneration, ExtensionSnapshot, ExtensionUpdate, HostGeneration,
    ReplaceableEvent,
};
pub use supervisor::{
    ChildSpawner, ExtensionHostDiagnostics, ExtensionSupervisor, SupervisorState,
};
pub use watcher::ExtensionWatcher;
