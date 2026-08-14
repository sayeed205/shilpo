pub mod manager;
pub mod manifest;
pub mod record;
pub mod runner;

pub use manager::{
    ScriptBundleInfo, ScriptClock, ScriptRuntime, SystemScriptClock, discover_script_bundles,
};
pub use manifest::{
    SCRIPT_SCHEMA_VERSION, ScriptBarWidgetContribution, ScriptContributions, ScriptManifest,
    ScriptManifestError, ScriptMode, ScriptRuntimeConfig,
};
pub use record::{MAX_RECORD_BYTES, ScriptRecord, ScriptRecordPayload, decode_and_validate_record};
pub use runner::{
    ProcessOutput, ProcessRunner, RealProcessRunner, ScriptProcessError, StreamProcess,
};

#[cfg(test)]
mod tests;
