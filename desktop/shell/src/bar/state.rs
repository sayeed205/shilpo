use crate::bar::service_worker::{ConfigUpdate, WorkerUpdate};
use crate::bar::widgets::clock::{format_clock, format_date};
use crate::osd::OsdKind;
use shilpo_config::ShellConfig;
use shilpo_services::{AudioInfo, BatteryInfo, BluetoothInfo, MediaInfo, NetworkInfo, Notification};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum BarStateEffect {
    ShowOsd(OsdKind),
    ShowNotificationToast(Notification),
    ApplyConfigTheme(ShellConfig),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BarUpdateResult {
    pub changed: bool,
    pub effects: Vec<BarStateEffect>,
}

pub struct BarState {
    pub config: ShellConfig,
    pub battery: BatteryInfo,
    pub audio: AudioInfo,
    pub network: NetworkInfo,
    pub bluetooth: BluetoothInfo,
    pub caffeine_service: Arc<shilpo_services::CaffeineService>,
    pub app_id: String,
    pub active_title: String,
    pub media_info: Option<MediaInfo>,
    pub time_str: String,
    pub date_str: String,
    pub cpu_percent: u8,
    pub ram_percent: u8,
    pub cat_frame_index: usize,
    pub last_error: Option<String>,
    pub last_service_update: Instant,
}

impl BarState {
    pub fn new(config: ShellConfig) -> Self {
        let battery = BatteryInfo::default();
        let audio = AudioInfo::default();
        let network = NetworkInfo::default();
        let bluetooth = shilpo_services::BluetoothService::new()
            .map(|s| s.info())
            .unwrap_or_default();
        let caffeine_service = Arc::new(shilpo_services::CaffeineService::new());

        let now = chrono::Local::now();
        let time_str = format_clock(&now, config.clock_format.as_deref());
        let date_str = format_date(&now);

        Self {
            config,
            battery,
            audio,
            network,
            bluetooth,
            caffeine_service,
            app_id: "shilpo.shell".into(),
            active_title: "Shilpo Shell".into(),
            media_info: None,
            time_str,
            date_str,
            cpu_percent: 0,
            ram_percent: 0,
            cat_frame_index: 0,
            last_error: None,
            last_service_update: Instant::now(),
        }
    }

    pub fn update_datetime(&mut self) -> bool {
        let now = chrono::Local::now();
        let fmt = self.config.clock_format.as_deref();
        let new_time = format_clock(&now, fmt);
        let new_date = format_date(&now);
        let mut changed = false;

        if self.time_str != new_time {
            self.time_str = new_time;
            changed = true;
        }
        if self.date_str != new_date {
            self.date_str = new_date;
            changed = true;
        }

        changed
    }

    pub fn is_stale(&self) -> bool {
        self.last_service_update.elapsed() > std::time::Duration::from_secs(30)
    }

    pub fn apply_worker_update(&mut self, update: &WorkerUpdate) -> BarUpdateResult {
        self.last_service_update = Instant::now();
        let mut changed = false;
        let mut effects = Vec::new();

        match update {
            WorkerUpdate::Battery(value) if &self.battery != value => {
                self.battery = value.clone();
                changed = true;
            }
            WorkerUpdate::Audio(value) if &self.audio != value => {
                let show_osd = self.audio.available
                    && value.available
                    && (self.audio.volume != value.volume || self.audio.is_muted != value.is_muted);
                self.audio = value.clone();
                if show_osd {
                    effects.push(BarStateEffect::ShowOsd(OsdKind::Volume {
                        level: value.volume as u32,
                        muted: value.is_muted,
                    }));
                }
                changed = true;
            }
            WorkerUpdate::Network(value) if &self.network != value => {
                self.network = value.clone();
                changed = true;
            }
            WorkerUpdate::Media(value) if self.media_info.as_ref() != Some(value) => {
                self.media_info = Some(value.clone());
                changed = true;
            }
            WorkerUpdate::Config(ConfigUpdate::Loaded(config)) => {
                self.config = (**config).clone();
                self.last_error = None;
                effects.push(BarStateEffect::ApplyConfigTheme(self.config.clone()));
                changed = self.update_datetime() || true;
            }
            WorkerUpdate::Config(ConfigUpdate::Failed(error)) => {
                tracing::error!(error = %error, "config reload failed");
                self.last_error = Some(error.clone());
                effects.push(BarStateEffect::ShowNotificationToast(Notification::new(
                    "Configuration Warning",
                    error,
                )));
                changed = true;
            }
            _ => {}
        }

        BarUpdateResult { changed, effects }
    }
}

impl Default for BarState {
    fn default() -> Self {
        Self::new(ShellConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_state_default_construction() {
        let state = BarState::default();
        assert_eq!(state.app_id, "shilpo.shell");
        assert_eq!(state.active_title, "Shilpo Shell");
        assert_eq!(state.cpu_percent, 0);
        assert_eq!(state.ram_percent, 0);
        assert_eq!(state.cat_frame_index, 0);
        assert_eq!(state.media_info, None);
        assert_eq!(state.last_error, None);
        assert!(!state.is_stale());
    }

    #[test]
    fn test_apply_worker_update_battery() {
        let mut state = BarState::default();
        let battery = BatteryInfo {
            is_present: true,
            percentage: 85,
            ..Default::default()
        };

        let result = state.apply_worker_update(&WorkerUpdate::Battery(battery.clone()));
        assert!(result.changed);
        assert_eq!(state.battery.percentage, 85);
        assert!(result.effects.is_empty());
    }

    #[test]
    fn test_apply_worker_update_audio_triggers_osd() {
        let mut state = BarState {
            audio: AudioInfo {
                available: true,
                volume: 50,
                is_muted: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let new_audio = AudioInfo {
            volume: 70,
            ..state.audio.clone()
        };

        let result = state.apply_worker_update(&WorkerUpdate::Audio(new_audio));
        assert!(result.changed);
        assert_eq!(state.audio.volume, 70);
        assert_eq!(
            result.effects,
            vec![BarStateEffect::ShowOsd(OsdKind::Volume {
                level: 70,
                muted: false,
            })]
        );
    }

    #[test]
    fn test_apply_worker_update_network() {
        let mut state = BarState::default();
        let net = NetworkInfo {
            is_connected: true,
            ssid: Some("WiFi-Home".into()),
            ..Default::default()
        };

        let result = state.apply_worker_update(&WorkerUpdate::Network(net.clone()));
        assert!(result.changed);
        assert_eq!(state.network.ssid.as_deref(), Some("WiFi-Home"));
    }

    #[test]
    fn test_apply_worker_update_media() {
        let mut state = BarState::default();
        let media = MediaInfo {
            player_id: "spotify".into(),
            title: "Song".into(),
            artist: "Artist".into(),
            art_url: "".into(),
            playback_state: shilpo_services::PlaybackState::Playing,
            can_play_pause: true,
            can_go_next: true,
            position_secs: 0.0,
            length_secs: 180.0,
        };

        let result = state.apply_worker_update(&WorkerUpdate::Media(media.clone()));
        assert!(result.changed);
        assert_eq!(state.media_info, Some(media));
    }

    #[test]
    fn test_apply_worker_update_config_loaded() {
        let mut state = BarState::default();
        let new_config = ShellConfig {
            clock_format: Some("%H:%M".into()),
            ..Default::default()
        };

        let result = state.apply_worker_update(&WorkerUpdate::Config(ConfigUpdate::Loaded(
            Box::new(new_config.clone()),
        )));
        assert!(result.changed);
        assert_eq!(state.config.clock_format.as_deref(), Some("%H:%M"));
        assert_eq!(
            result.effects,
            vec![BarStateEffect::ApplyConfigTheme(new_config)]
        );
    }

    #[test]
    fn test_apply_worker_update_config_failed() {
        let mut state = BarState::default();
        let err_msg = "Invalid TOML syntax".to_string();

        let result = state.apply_worker_update(&WorkerUpdate::Config(ConfigUpdate::Failed(
            err_msg.clone(),
        )));
        assert!(result.changed);
        assert_eq!(state.last_error, Some(err_msg.clone()));
        assert_eq!(result.effects.len(), 1);
        if let BarStateEffect::ShowNotificationToast(notif) = &result.effects[0] {
            assert_eq!(notif.summary, "Configuration Warning");
            assert_eq!(notif.body, err_msg);
        } else {
            panic!("expected ShowNotificationToast effect");
        }
    }
}
