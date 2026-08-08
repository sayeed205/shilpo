use crate::backend::create_backend;
use crate::types::RecordingSource;

/// Enumerate available video capture sources (outputs, windows, regions)
pub fn enumerate_sources() -> anyhow::Result<Vec<RecordingSource>> {
    let backend = create_backend()?;
    backend.enumerate_sources()
}
