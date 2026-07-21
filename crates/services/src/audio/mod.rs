use anyhow::Result;
use std::{
    process::Command,
    sync::{Arc, Mutex},
};

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
        let service = Self { info };

        let info_clone = service.info.clone();
        tokio::spawn(async move {
            loop {
                let volume = query_volume().unwrap_or(75);
                let is_muted = query_mute().unwrap_or(false);
                {
                    let mut lock = info_clone.lock().unwrap();
                    *lock = AudioInfo { volume, is_muted };
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
        });

        Ok(service)
    }

    pub fn audio_info(&self) -> AudioInfo {
        self.info.lock().unwrap().clone()
    }
}

fn query_volume() -> Option<u8> {
    let output = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
        .ok()?;

    if output.status.success() {
        let out_str = String::from_utf8_lossy(&output.stdout);
        // Format: Volume: front-left: 49152 /  75% / -7.53 dB, ...
        for word in out_str.split_whitespace() {
            if word.ends_with('%')
                && let Ok(vol) = word.trim_end_matches('%').parse::<u8>()
            {
                return Some(vol);
            }
        }
    }
    None
}

fn query_mute() -> Option<bool> {
    let output = Command::new("pactl")
        .args(["get-sink-mute", "@DEFAULT_SINK@"])
        .output()
        .ok()?;

    if output.status.success() {
        let out_str = String::from_utf8_lossy(&output.stdout);
        // Format: Mute: yes / Mute: no
        return Some(out_str.contains("yes"));
    }
    None
}
