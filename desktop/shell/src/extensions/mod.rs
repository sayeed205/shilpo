//! Shell-owned extension lifecycle, background worker engine, and coordinator.
//!
//! Surface code talks to `ExtensionCoordinator` instead of the WASM runtime or host engine.
//! UI views read prevalidated, immutable `ExtensionSnapshot`s without calling Wasmtime
//! or scanning package filesystems during rendering.

pub mod coordinator;
pub mod engine;
pub mod process;
pub mod supervisor;
pub mod watcher;

pub use coordinator::{
    ContributionDescriptor, ContributionInstance, ContributionSurface, ExtensionChanges,
    ExtensionCommand, ExtensionCoordinator, ExtensionGeneration, ExtensionSnapshot,
    ExtensionUpdate, ReplaceableEvent,
};
pub use engine::ExtensionEngine;
pub use process::{
    FrameReader, HostGeneration, HostMessage, PROTOCOL_VERSION, ProcessCodecError, WorkerMessage,
    WorkerPayload, recv_host_message, recv_worker_message, recv_worker_message_nonblocking,
    run_extension_host, send_host_message, send_worker_message,
};
pub use supervisor::{
    ChildSpawner, ExtensionHostDiagnostics, ExtensionSupervisor, SupervisorState,
};
pub use watcher::ExtensionWatcher;
