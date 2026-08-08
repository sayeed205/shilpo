use anyhow::Result;
#[cfg(target_os = "linux")]
use futures_lite::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::watch;

#[cfg(target_os = "linux")]
use zbus::proxy;

#[cfg(target_os = "linux")]
const MPRIS_PLAYER_PREFIX: &str = "org.mpris.MediaPlayer2.";

/// MPRIS playback state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// Active media track and player info.
/// Active media track and player info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub player_id: String,
    pub title: String,
    pub artist: String,
    pub art_url: String,
    pub playback_state: PlaybackState,
    pub can_play_pause: bool,
    pub can_go_next: bool,
    pub position_secs: f64,
    pub length_secs: f64,
    #[serde(default = "default_rate")]
    pub rate: f64,
    #[serde(skip)]
    pub observed_at: Option<std::time::Instant>,
}

fn default_rate() -> f64 {
    1.0
}

impl Default for MediaInfo {
    fn default() -> Self {
        Self {
            player_id: String::new(),
            title: String::new(),
            artist: String::new(),
            art_url: String::new(),
            playback_state: PlaybackState::Stopped,
            can_play_pause: false,
            can_go_next: false,
            position_secs: 0.0,
            length_secs: 0.0,
            rate: 1.0,
            observed_at: None,
        }
    }
}

impl MediaInfo {
    pub fn is_empty(&self) -> bool {
        self.player_id.is_empty() || (self.title.is_empty() && self.artist.is_empty())
    }

    pub fn current_position_secs(&self) -> f64 {
        if self.playback_state == PlaybackState::Playing {
            if let Some(obs) = self.observed_at {
                let elapsed = obs.elapsed().as_secs_f64();
                let extrapolated = self.position_secs + (elapsed * self.rate);
                if self.length_secs > 0.0 {
                    extrapolated.min(self.length_secs)
                } else {
                    extrapolated
                }
            } else {
                self.position_secs
            }
        } else {
            self.position_secs
        }
    }

    pub fn progress(&self) -> f32 {
        if self.length_secs > 0.0 {
            (self.current_position_secs() / self.length_secs).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
}

/// Media control commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaCommand {
    PlayPause,
    Next,
}

#[cfg(target_os = "linux")]
#[proxy(
    interface = "org.freedesktop.DBus",
    default_service = "org.freedesktop.DBus",
    default_path = "/org/freedesktop/DBus"
)]
pub trait DBusDaemon {
    fn list_names(&self) -> zbus::Result<Vec<String>>;

    #[zbus(signal)]
    fn name_owner_changed(name: &str, old_owner: &str, new_owner: &str) -> zbus::Result<()>;
}

#[cfg(target_os = "linux")]
#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
pub trait MprisPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, zbus::zvariant::OwnedValue>>;

    #[zbus(property)]
    fn can_play(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn can_pause(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn can_go_next(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;

    #[zbus(property)]
    fn rate(&self) -> zbus::Result<f64>;

    #[zbus(signal)]
    fn seeked(&self, position: i64) -> zbus::Result<()>;

    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
}

/// Helper function to parse MPRIS metadata dictionary into title, artist, art_url, length_secs, and position_secs.
pub fn parse_mpris_metadata(
    #[cfg(target_os = "linux")] map: &HashMap<String, zbus::zvariant::OwnedValue>,
    #[cfg(not(target_os = "linux"))] map: &HashMap<String, String>,
) -> (String, String, String, f64, f64) {
    let mut title = String::new();
    let mut artist = String::new();
    let mut art_url = String::new();
    let mut length_secs = 0.0;
    let mut position_secs = 0.0;

    #[cfg(target_os = "linux")]
    {
        if let Some(val) = map.get("xesam:title") {
            let inner: &zbus::zvariant::Value = val;
            if let Ok(s) = <&str>::try_from(inner) {
                title = s.to_string();
            } else if let Ok(s) = <String>::try_from(inner) {
                title = s;
            }
        }

        if let Some(val) = map.get("xesam:artist") {
            let inner: &zbus::zvariant::Value = val;
            if let Ok(s) = <&str>::try_from(inner) {
                artist = s.to_string();
            } else if let Ok(s) = <String>::try_from(inner) {
                artist = s;
            } else if let zbus::zvariant::Value::Array(arr) = inner {
                let parts: Vec<String> = arr
                    .iter()
                    .filter_map(|item| <&str>::try_from(item).ok().map(|s| s.to_string()))
                    .collect();
                if !parts.is_empty() {
                    artist = parts.join(", ");
                }
            }
        }

        if let Some(val) = map.get("mpris:artUrl") {
            let inner: &zbus::zvariant::Value = val;
            if let Ok(s) = <&str>::try_from(inner) {
                art_url = s.to_string();
            } else if let Ok(s) = <String>::try_from(inner) {
                art_url = s;
            }
        }

        if let Some(val) = map.get("mpris:length") {
            let inner: &zbus::zvariant::Value = val;
            if let Ok(l) = <i64>::try_from(inner) {
                length_secs = l as f64 / 1_000_000.0;
            } else if let Ok(l) = <u64>::try_from(inner) {
                length_secs = l as f64 / 1_000_000.0;
            }
        }

        if let Some(val) = map.get("mpris:position") {
            let inner: &zbus::zvariant::Value = val;
            if let Ok(l) = <i64>::try_from(inner) {
                position_secs = l as f64 / 1_000_000.0;
            } else if let Ok(l) = <u64>::try_from(inner) {
                position_secs = l as f64 / 1_000_000.0;
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        if let Some(s) = map.get("xesam:title") {
            title = s.clone();
        }
        if let Some(s) = map.get("xesam:artist") {
            artist = s.clone();
        }
        if let Some(s) = map.get("mpris:artUrl") {
            art_url = s.clone();
        }
    }

    (title, artist, art_url, length_secs, position_secs)
}

fn candidate_score(info: &MediaInfo) -> u32 {
    let mut score = 0;
    match info.playback_state {
        PlaybackState::Playing => score += 1000,
        PlaybackState::Paused => score += 500,
        PlaybackState::Stopped => return 0,
    }

    if !info.artist.is_empty() {
        score += 100;
    }
    if !info.art_url.is_empty() {
        score += 200;
    }
    if !info.title.is_empty() {
        score += 50;
    }
    if info.player_id.contains("plasma-browser-integration") {
        score += 20;
    }

    score
}

/// Helper function to rank and select the best candidate player among available player infos.
/// Priority:
/// 1. Active playback status (Playing > Paused).
/// 2. Rich metadata quality (Artist + Artwork + Title present).
/// 3. Browser extension integration over bare browser process.
pub fn select_best_player(candidates: Vec<MediaInfo>) -> MediaInfo {
    candidates
        .into_iter()
        .filter(|info| !info.is_empty())
        .max_by_key(candidate_score)
        .unwrap_or_default()
}

use crate::runtime::{CommandContext, CommandRuntime, StateContext};

/// MPRIS session D-Bus service for controlling and observing media playback.
#[derive(Clone)]
pub struct MediaService {
    runtime: CommandRuntime<MediaInfo, MediaCommand>,
}

impl Default for MediaService {
    fn default() -> Self {
        Self::new_offline()
    }
}

#[cfg(target_os = "linux")]
async fn fetch_and_publish_media(
    connection: &zbus::Connection,
    daemon_proxy: &DBusDaemonProxy<'_>,
    state: &StateContext<MediaInfo>,
) -> Option<String> {
    let names = daemon_proxy.list_names().await.ok()?;
    let player_names: Vec<String> = names
        .into_iter()
        .filter(|name| name.starts_with(MPRIS_PLAYER_PREFIX))
        .collect();

    let mut candidates = Vec::new();
    let now = std::time::Instant::now();

    for bus_name in &player_names {
        if let Ok(builder) = MprisPlayerProxy::builder(connection).destination(bus_name.as_str())
            && let Ok(player_proxy) = builder.build().await
        {
            let status_str = player_proxy
                .playback_status()
                .await
                .unwrap_or_else(|_| "Stopped".to_string());
            let playback_state = match status_str.as_str() {
                "Playing" => PlaybackState::Playing,
                "Paused" => PlaybackState::Paused,
                _ => PlaybackState::Stopped,
            };

            let meta_map = player_proxy.metadata().await.unwrap_or_default();
            let (title, artist, art_url, length_secs, meta_pos_secs) =
                parse_mpris_metadata(&meta_map);

            let dbus_pos_secs = player_proxy
                .position()
                .await
                .map(|p| p as f64 / 1_000_000.0)
                .unwrap_or(0.0);

            let position_secs = if dbus_pos_secs > 0.0 {
                dbus_pos_secs
            } else {
                meta_pos_secs
            };

            let rate = player_proxy.rate().await.unwrap_or(1.0);
            let can_play = player_proxy.can_play().await.unwrap_or(false);
            let can_pause = player_proxy.can_pause().await.unwrap_or(false);
            let can_go_next = player_proxy.can_go_next().await.unwrap_or(false);

            candidates.push(MediaInfo {
                player_id: bus_name.clone(),
                title,
                artist,
                art_url,
                playback_state,
                can_play_pause: can_play || can_pause,
                can_go_next,
                position_secs,
                length_secs,
                rate,
                observed_at: Some(now),
            });
        }
    }

    let previous = state.get();
    let mut best = select_best_player(candidates);
    if best.is_empty() && player_names.iter().any(|name| name == &previous.player_id) {
        best = previous;
    }
    let active_player = if best.player_id.is_empty() {
        None
    } else {
        Some(best.player_id.clone())
    };

    state.send_replace(best);
    active_player
}

#[cfg(target_os = "linux")]
async fn run_mpris_loop(mut ctx: CommandContext<MediaInfo, MediaCommand>) {
    loop {
        let connection = match zbus::Connection::session().await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::debug!("MPRIS D-Bus session connection failed: {err}; retrying");
                ctx.state.send_replace(MediaInfo::default());
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        let daemon_proxy = match DBusDaemonProxy::new(&connection).await {
            Ok(proxy) => proxy,
            Err(err) => {
                tracing::debug!("DBusDaemonProxy failed: {err}; retrying");
                ctx.state.send_replace(MediaInfo::default());
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        // Add D-Bus match rules so signal events fire for properties, seek, and name owner changes
        if let Ok(dbus_fdo) = zbus::fdo::DBusProxy::new(&connection).await {
            if let Ok(rule) = zbus::MatchRule::try_from(
                "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'",
            ) {
                let _ = dbus_fdo.add_match_rule(rule).await;
            }
            if let Ok(rule) = zbus::MatchRule::try_from(
                "type='signal',interface='org.mpris.MediaPlayer2.Player',member='Seeked'",
            ) {
                let _ = dbus_fdo.add_match_rule(rule).await;
            }
            if let Ok(rule) = zbus::MatchRule::try_from(
                "type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0namespace='org.mpris.MediaPlayer2'",
            ) {
                let _ = dbus_fdo.add_match_rule(rule).await;
            }
        }

        let mut selected_player_name =
            fetch_and_publish_media(&connection, &daemon_proxy, &ctx.state).await;

        let mut stream = zbus::MessageStream::from(&connection);

        loop {
            tokio::select! {
                cmd = ctx.command_rx.recv() => {
                    match cmd {
                        Some(cmd) => {
                            if let Some(ref bus_name) = selected_player_name
                                && let Ok(builder) = MprisPlayerProxy::builder(&connection)
                                    .destination(bus_name.as_str())
                                && let Ok(player_proxy) = builder.build().await
                            {
                                match cmd {
                                    MediaCommand::PlayPause => {
                                        let _ = player_proxy.play_pause().await;
                                    }
                                    MediaCommand::Next => {
                                        let _ = player_proxy.next().await;
                                    }
                                }
                            }
                            selected_player_name =
                                fetch_and_publish_media(&connection, &daemon_proxy, &ctx.state).await;
                        }
                        None => return,
                    }
                }
                msg = stream.next() => {
                    if msg.is_some() {
                        selected_player_name =
                            fetch_and_publish_media(&connection, &daemon_proxy, &ctx.state).await;
                    } else {
                        break;
                    }
                }
            }
        }

        ctx.state.send_replace(MediaInfo::default());
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

impl MediaService {
    pub fn new_offline() -> Self {
        let runtime = CommandRuntime::new_offline(MediaInfo::default());
        Self { runtime }
    }

    pub fn new() -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            let runtime = CommandRuntime::spawn(MediaInfo::default(), run_mpris_loop);
            Ok(Self { runtime })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self::new_offline())
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<MediaInfo> {
        self.runtime.subscribe()
    }

    pub fn media_info(&self) -> MediaInfo {
        self.runtime.get()
    }

    pub fn send_command(&self, command: MediaCommand) {
        let _ = self.runtime.send_command(command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_info_is_empty() {
        let empty = MediaInfo::default();
        assert!(empty.is_empty());

        let with_title = MediaInfo {
            player_id: "org.mpris.MediaPlayer2.spotify".into(),
            title: "Song Title".into(),
            artist: "".into(),
            ..Default::default()
        };
        assert!(!with_title.is_empty());

        let with_artist = MediaInfo {
            player_id: "org.mpris.MediaPlayer2.vlc".into(),
            title: "".into(),
            artist: "Artist Name".into(),
            ..Default::default()
        };
        assert!(!with_artist.is_empty());
    }

    #[test]
    fn test_player_selection_priority_playing_over_paused() {
        let playing = MediaInfo {
            player_id: "org.mpris.MediaPlayer2.spotify".into(),
            title: "Playing Song".into(),
            artist: "Playing Artist".into(),
            playback_state: PlaybackState::Playing,
            can_play_pause: true,
            can_go_next: true,
            ..Default::default()
        };

        let paused = MediaInfo {
            player_id: "org.mpris.MediaPlayer2.vlc".into(),
            title: "Paused Song".into(),
            artist: "Paused Artist".into(),
            playback_state: PlaybackState::Paused,
            can_play_pause: true,
            can_go_next: false,
            ..Default::default()
        };

        let candidates = vec![paused.clone(), playing.clone()];
        let best = select_best_player(candidates);
        assert_eq!(best.player_id, "org.mpris.MediaPlayer2.spotify");
        assert_eq!(best.playback_state, PlaybackState::Playing);
    }

    #[test]
    fn test_player_selection_fallback_to_paused() {
        let paused = MediaInfo {
            player_id: "org.mpris.MediaPlayer2.vlc".into(),
            title: "Paused Song".into(),
            artist: "Paused Artist".into(),
            playback_state: PlaybackState::Paused,
            can_play_pause: true,
            can_go_next: true,
            ..Default::default()
        };

        let stopped = MediaInfo {
            player_id: "org.mpris.MediaPlayer2.mpv".into(),
            title: "Stopped Song".into(),
            artist: "Stopped Artist".into(),
            playback_state: PlaybackState::Stopped,
            ..Default::default()
        };

        let candidates = vec![stopped, paused.clone()];
        let best = select_best_player(candidates);
        assert_eq!(best.player_id, "org.mpris.MediaPlayer2.vlc");
        assert_eq!(best.playback_state, PlaybackState::Paused);
    }

    #[test]
    fn test_player_selection_empty_when_no_usable_players() {
        let candidates = vec![MediaInfo {
            player_id: "org.mpris.MediaPlayer2.mpv".into(),
            title: "".into(),
            artist: "".into(),
            playback_state: PlaybackState::Stopped,
            ..Default::default()
        }];
        let best = select_best_player(candidates);
        assert!(best.is_empty());
    }

    #[test]
    fn test_parse_mpris_metadata() {
        let mut map = HashMap::new();
        #[cfg(not(target_os = "linux"))]
        {
            map.insert("xesam:title".to_string(), "Bohemian Rhapsody".to_string());
            map.insert("xesam:artist".to_string(), "Queen".to_string());
            map.insert(
                "mpris:artUrl".to_string(),
                "https://example.com/art.jpg".to_string(),
            );
        }

        #[cfg(target_os = "linux")]
        {
            map.insert(
                "xesam:title".to_string(),
                zbus::zvariant::Value::from("Bohemian Rhapsody")
                    .try_into()
                    .unwrap(),
            );
            map.insert(
                "xesam:artist".to_string(),
                zbus::zvariant::Value::from(vec!["Queen"])
                    .try_into()
                    .unwrap(),
            );
            map.insert(
                "mpris:artUrl".to_string(),
                zbus::zvariant::Value::from("https://example.com/art.jpg")
                    .try_into()
                    .unwrap(),
            );
        }

        let (title, artist, art_url, _, _) = parse_mpris_metadata(&map);
        assert_eq!(title, "Bohemian Rhapsody");
        assert_eq!(artist, "Queen");
        assert_eq!(art_url, "https://example.com/art.jpg");
    }

    #[test]
    fn test_player_selection_prefers_rich_metadata() {
        let bare_chromium = MediaInfo {
            player_id: "org.mpris.MediaPlayer2.chromium.instance123".into(),
            title: "Some Tab Title - YouTube".into(),
            artist: "".into(),
            art_url: "".into(),
            playback_state: PlaybackState::Paused,
            can_play_pause: true,
            can_go_next: true,
            ..Default::default()
        };

        let plasma_integration = MediaInfo {
            player_id: "org.mpris.MediaPlayer2.plasma-browser-integration".into(),
            title: "Clean Track Title".into(),
            artist: "Fireship".into(),
            art_url: "file:///tmp/art.jpg".into(),
            playback_state: PlaybackState::Paused,
            can_play_pause: true,
            can_go_next: true,
            ..Default::default()
        };

        let candidates = vec![bare_chromium, plasma_integration];
        let best = select_best_player(candidates);
        assert_eq!(
            best.player_id,
            "org.mpris.MediaPlayer2.plasma-browser-integration"
        );
        assert_eq!(best.artist, "Fireship");
    }

    #[test]
    fn test_position_extrapolation_when_playing() {
        let info = MediaInfo {
            player_id: "spotify".into(),
            title: "Test".into(),
            playback_state: PlaybackState::Playing,
            position_secs: 10.0,
            length_secs: 100.0,
            rate: 1.0,
            observed_at: Some(std::time::Instant::now() - std::time::Duration::from_secs(2)),
            ..Default::default()
        };

        let curr = info.current_position_secs();
        assert!((11.9..=13.0).contains(&curr));
        assert!(info.progress() > 0.11);
    }

    #[test]
    fn test_position_not_extrapolated_when_paused() {
        let info = MediaInfo {
            player_id: "spotify".into(),
            title: "Test".into(),
            playback_state: PlaybackState::Paused,
            position_secs: 10.0,
            length_secs: 100.0,
            rate: 1.0,
            observed_at: Some(std::time::Instant::now() - std::time::Duration::from_secs(5)),
            ..Default::default()
        };

        assert_eq!(info.current_position_secs(), 10.0);
        assert_eq!(info.progress(), 0.1);
    }
}
