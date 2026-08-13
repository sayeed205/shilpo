pub mod manager;
pub mod manifest;
pub mod record;
pub mod runner;

pub use manager::{ScriptBundleInfo, ScriptBundleState, ScriptRuntime, discover_script_bundles};
pub use manifest::{
    ScriptBarWidgetContribution, ScriptContributions, ScriptManifest, ScriptMode,
    ScriptRuntimeConfig,
};
pub use record::{MAX_RECORD_BYTES, ScriptRecord, ScriptRecordPayload, decode_and_validate_record};
pub use runner::{ProcessOutput, ProcessRunner, RealProcessRunner, StreamProcess};

#[cfg(test)]
mod tests;
