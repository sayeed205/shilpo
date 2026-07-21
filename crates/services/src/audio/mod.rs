use anyhow::Result;
use std::sync::{Arc, Mutex};

/// Audio sink volume & mute status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioInfo {
    pub volume: u8,
    pub is_muted: bool,
}

impl Default for AudioInfo {
    fn default() -> Self {
        Self {
            volume: 75,
            is_muted: false,
        }
    }
}

/// System audio service for volume and mute status.
pub struct AudioService {
    info: Arc<Mutex<AudioInfo>>,
}

impl AudioService {
    pub fn new() -> Result<Self> {
        let info = Arc::new(Mutex::new(AudioInfo::default()));
        Ok(Self { info })
    }

    pub fn audio_info(&self) -> AudioInfo {
        self.info.lock().unwrap().clone()
    }
}
