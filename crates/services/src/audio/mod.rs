use anyhow::Result;
use std::{
    process::Command,
    sync::{Arc, Mutex},
};

/// Audio sink volume & mute status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioInfo {
    pub volume: u8,
    pub is_muted: bool,
    pub available: bool,
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
                let info = match (query_volume(), query_mute()) {
                    (Some(volume), Some(is_muted)) => AudioInfo {
                        volume,
                        is_muted,
                        available: true,
                    },
                    _ => AudioInfo::default(),
                };
                *info_clone.lock().unwrap() = info;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unavailable() {
        assert_eq!(AudioInfo::default(), AudioInfo::default());
        assert_eq!(AudioInfo::default().volume, 0);
        assert!(!AudioInfo::default().is_muted);
        assert!(!AudioInfo::default().available);
    }
}
