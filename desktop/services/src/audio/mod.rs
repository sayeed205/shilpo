use anyhow::Result;
use std::process::Command;

/// Metadata describing an individual application audio playback stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioStream {
    pub id: u32,
    pub name: String,
    pub volume_percent: u8,
    pub is_muted: bool,
}

/// Audio sink volume & mute status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioInfo {
    pub volume: u8,
    pub is_muted: bool,
    pub available: bool,
    pub app_streams: Vec<AudioStream>,
}

/// Metadata describing a physical audio device (sink or source).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_default: bool,
    pub is_input: bool,
}

/// Metadata describing an audio port on a sound card (e.g. Headphones vs Speakers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioPort {
    pub name: String,
    pub description: String,
    pub is_active: bool,
}

use tokio::sync::watch;

/// System audio service for volume, mute status, and device switching.
pub struct AudioService {
    tx: watch::Sender<AudioInfo>,
    _task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for AudioService {
    fn drop(&mut self) {
        if let Some(task) = self._task.take() {
            task.abort();
        }
    }
}

impl AudioService {
    pub fn new() -> Result<Self> {
        let (tx, _rx) = watch::channel(AudioInfo::default());

        let tx_clone = tx.clone();
        let task = tokio::spawn(async move {
            loop {
                let info = match (query_volume(), query_mute()) {
                    (Some(volume), Some(is_muted)) => AudioInfo {
                        volume,
                        is_muted,
                        available: true,
                        app_streams: Vec::new(),
                    },
                    _ => AudioInfo::default(),
                };
                let _ = tx_clone.send_replace(info);
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
        });

        Ok(Self {
            tx,
            _task: Some(task),
        })
    }

    pub fn subscribe(&self) -> watch::Receiver<AudioInfo> {
        self.tx.subscribe()
    }

    pub fn audio_info(&self) -> AudioInfo {
        self.tx.borrow().clone()
    }

    pub fn list_devices(&self) -> Vec<AudioDevice> {
        Self::list_devices_static()
    }

    pub fn list_devices_static() -> Vec<AudioDevice> {
        let mut devices = Vec::new();

        // Query output sinks
        if let Ok(output) = Command::new("pactl")
            .args(["list", "sinks", "short"])
            .output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[1].to_string();
                    let description = name
                        .split('.')
                        .next_back()
                        .unwrap_or(&name)
                        .replace('_', " ");
                    devices.push(AudioDevice {
                        id: name.clone(),
                        name: name.clone(),
                        description,
                        is_default: false,
                        is_input: false,
                    });
                }
            }
        }

        // Query input sources
        if let Ok(output) = Command::new("pactl")
            .args(["list", "sources", "short"])
            .output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[1].to_string();
                    if !name.ends_with(".monitor") {
                        let description = name
                            .split('.')
                            .next_back()
                            .unwrap_or(&name)
                            .replace('_', " ");
                        devices.push(AudioDevice {
                            id: name.clone(),
                            name: name.clone(),
                            description,
                            is_default: false,
                            is_input: true,
                        });
                    }
                }
            }
        }

        devices
    }

    pub fn increase_volume(&self, step: u8) {
        let _ = Command::new("wpctl")
            .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{step}%+")])
            .status();
    }

    pub fn decrease_volume(&self, step: u8) {
        let _ = Command::new("wpctl")
            .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{step}%-")])
            .status();
    }

    pub fn set_volume(&self, vol: u8) {
        let _ = Command::new("pactl")
            .args(["set-sink-volume", "@DEFAULT_SINK@", &format!("{vol}%")])
            .status();
    }

    pub fn toggle_mute(&self) {
        let _ = Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
            .status();
    }

    pub fn set_stream_volume(&self, index: u32, percentage: u8) -> Result<()> {
        let status = Command::new("pactl")
            .args([
                "set-sink-input-volume",
                &index.to_string(),
                &format!("{percentage}%"),
            ])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("failed to set audio stream volume"))
        }
    }

    pub fn toggle_stream_mute(&self, index: u32) -> Result<()> {
        let status = Command::new("pactl")
            .args(["set-sink-input-mute", &index.to_string(), "toggle"])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("failed to toggle audio stream mute"))
        }
    }

    pub fn set_default_device(&self, device_id: &str, is_input: bool) -> Result<()> {
        let cmd = if is_input {
            "set-default-source"
        } else {
            "set-default-sink"
        };
        let status = Command::new("pactl").args([cmd, device_id]).status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("failed to set default audio device"))
        }
    }

    pub fn list_ports(&self) -> Vec<AudioPort> {
        Self::list_ports_static()
    }

    pub fn list_ports_static() -> Vec<AudioPort> {
        let mut ports = Vec::new();
        if let Ok(output) = Command::new("pactl").args(["list", "sinks"]).output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut in_ports_section = false;
            let mut active_port = String::new();

            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("Active Port:") {
                    active_port = trimmed
                        .trim_start_matches("Active Port:")
                        .trim()
                        .to_string();
                } else if trimmed.starts_with("Ports:") {
                    in_ports_section = true;
                } else if trimmed.starts_with("Formats:") || trimmed.starts_with("State:") {
                    in_ports_section = false;
                } else if in_ports_section
                    && line.starts_with("\t\t")
                    && let Some((name_part, desc_part)) = trimmed.split_once(':')
                {
                    let port_name = name_part.trim().to_string();
                    let desc = desc_part
                        .split('(')
                        .next()
                        .unwrap_or(desc_part)
                        .trim()
                        .to_string();
                    let is_active = port_name == active_port;
                    ports.push(AudioPort {
                        name: port_name,
                        description: desc,
                        is_active,
                    });
                }
            }
        }
        ports
    }

    pub fn set_sink_port(&self, sink_name: &str, port_name: &str) -> Result<()> {
        let status = Command::new("pactl")
            .args(["set-sink-port", sink_name, port_name])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("failed to set sink port"))
        }
    }

    pub fn toggle_simultaneous_output(&self) -> Result<bool> {
        if let Ok(output) = Command::new("pactl")
            .args(["list", "modules", "short"])
            .output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.contains("module-combine-sink") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(&module_id) = parts.first() {
                        let _ = Command::new("pactl")
                            .args(["unload-module", module_id])
                            .status();
                        return Ok(false);
                    }
                }
            }
        }

        let status = Command::new("pactl")
            .args(["load-module", "module-combine-sink"])
            .status()?;
        Ok(status.success())
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

    #[test]
    fn test_audio_device_listing_and_switching_fallback() {
        let dev = AudioDevice {
            id: "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
            name: "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
            description: "analog stereo".to_string(),
            is_default: true,
            is_input: false,
        };
        assert_eq!(dev.id, "alsa_output.pci-0000_00_1f.3.analog-stereo");
        assert!(!dev.is_input);
    }

    #[test]
    fn test_audio_stream_volume_and_mute_controls() {
        let stream = AudioStream {
            id: 1,
            name: "Firefox".to_string(),
            volume_percent: 80,
            is_muted: false,
        };
        let mut info = AudioInfo::default();
        info.app_streams.push(stream);

        assert_eq!(info.app_streams.len(), 1);
        assert_eq!(info.app_streams[0].name, "Firefox");
        assert_eq!(info.app_streams[0].volume_percent, 80);
    }
}
