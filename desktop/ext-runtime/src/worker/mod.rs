pub mod engine;
pub mod process;
pub mod protocol;

pub use engine::{ActiveSource, ExtensionEngine, ExtensionSession};
pub use process::{
    run_extension_host, FrameReader, HostGeneration, HostMessage, ProcessCodecError,
    WorkerMessage, WorkerPayload, MAX_FRAME_SIZE, MAX_QUEUE_BOUND, PROTOCOL_VERSION,
    read_frame, recv_host_message, recv_worker_message, recv_worker_message_nonblocking,
    send_host_message, send_worker_message, write_frame,
};
pub use protocol::{
    ContributionDescriptor, ContributionInstance, ContributionSurface, ExtensionChanges,
    ExtensionCommand, ExtensionGeneration, ExtensionSnapshot, ExtensionUpdate, ReplaceableEvent,
};
