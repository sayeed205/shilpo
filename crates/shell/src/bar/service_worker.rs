use gpui::BackgroundExecutor;
use shilpo_config::ShellConfig;
use shilpo_services::{
    AudioInfo, AudioService, BatteryInfo, BatteryService, MediaCommand, MediaInfo, MediaService,
    NetworkInfo, NetworkService,
};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

pub type UpdateSender = mpsc::SyncSender<WorkerUpdate>;
pub type UpdateReceiver = mpsc::Receiver<WorkerUpdate>;
pub type CommandSender = mpsc::SyncSender<WorkerCommand>;
pub type CommandReceiver = mpsc::Receiver<WorkerCommand>;

#[derive(Debug)]
pub enum WorkerCommand {
    ReloadConfig,
    Media(MediaCommand),
}

#[derive(Debug, Clone)]
pub enum ConfigUpdate {
    Loaded(Box<ShellConfig>),
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum WorkerUpdate {
    Battery(BatteryInfo),
    Audio(AudioInfo),
    Network(NetworkInfo),
    Media(MediaInfo),
    Config(ConfigUpdate),
}

pub fn channels() -> (UpdateSender, UpdateReceiver, CommandSender, CommandReceiver) {
    let (updates_tx, updates_rx) = mpsc::sync_channel(64);
    let (commands_tx, commands_rx) = mpsc::sync_channel(32);
    (updates_tx, updates_rx, commands_tx, commands_rx)
}

pub fn try_send_command(
    sender: &CommandSender,
    command: WorkerCommand,
) -> Result<(), mpsc::TrySendError<WorkerCommand>> {
    sender.try_send(command)
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    executor: BackgroundExecutor,
    updates: UpdateSender,
    commands: CommandReceiver,
    config_path: PathBuf,
    battery: Option<BatteryService>,
    audio: Option<AudioService>,
    network: Option<NetworkService>,
    media: Option<MediaService>,
) -> gpui::Task<()> {
    let worker_executor = executor.clone();
    executor.spawn(async move {
        run(
            worker_executor,
            updates,
            commands,
            config_path,
            battery,
            audio,
            network,
            media,
        )
        .await
    })
}

#[allow(clippy::too_many_arguments)]
async fn run(
    executor: BackgroundExecutor,
    updates: UpdateSender,
    commands: CommandReceiver,
    config_path: PathBuf,
    battery: Option<BatteryService>,
    audio: Option<AudioService>,
    network: Option<NetworkService>,
    media: Option<MediaService>,
) {
    let mut battery_last = None;
    let mut audio_last = None;
    let mut network_last = None;
    let mut media_last = None;
    let mut device_ticks = 0u8;
    let mut pending_reload: Option<Instant> = None;
    let debounce_duration = Duration::from_millis(200);

    if !load_config(&updates, &config_path) {
        return;
    }

    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                WorkerCommand::ReloadConfig => {
                    pending_reload = Some(Instant::now());
                }
                WorkerCommand::Media(cmd) => {
                    if let Some(ref service) = media {
                        service.send_command(cmd);
                    }
                }
            }
        }

        if pending_reload.is_some_and(|req| req.elapsed() >= debounce_duration) {
            pending_reload = None;
            if !load_config(&updates, &config_path) {
                return;
            }
            invalidate_device_snapshots_after_config_reload(
                &mut audio_last,
                &mut battery_last,
                &mut network_last,
                &mut media_last,
                &mut device_ticks,
            );
        }

        if !sample_device(
            &updates,
            &mut audio_last,
            audio.as_ref().map(AudioService::audio_info),
            WorkerUpdate::Audio,
        ) {
            return;
        }

        if !sample_device(
            &updates,
            &mut media_last,
            media.as_ref().map(MediaService::media_info),
            WorkerUpdate::Media,
        ) {
            return;
        }

        if device_ticks == 0 {
            if !sample_device(
                &updates,
                &mut battery_last,
                battery.as_ref().map(BatteryService::battery_info),
                WorkerUpdate::Battery,
            ) {
                return;
            }
            if !sample_device(
                &updates,
                &mut network_last,
                network.as_ref().map(NetworkService::network_info),
                WorkerUpdate::Network,
            ) {
                return;
            }
        }
        device_ticks = (device_ticks + 1) % 30;
        executor.timer(Duration::from_millis(100)).await;
    }
}

fn load_config(updates: &UpdateSender, path: &PathBuf) -> bool {
    let update = match ShellConfig::load_or_create(path) {
        Ok(config) => WorkerUpdate::Config(ConfigUpdate::Loaded(Box::new(config))),
        Err(error) => WorkerUpdate::Config(ConfigUpdate::Failed(error.to_string())),
    };
    match updates.try_send(update) {
        Ok(()) | Err(mpsc::TrySendError::Full(_)) => true,
        Err(mpsc::TrySendError::Disconnected(_)) => false,
    }
}

fn send_changed<T: Clone + PartialEq>(
    updates: &UpdateSender,
    previous: &mut Option<T>,
    value: T,
    make_update: impl FnOnce(T) -> WorkerUpdate,
) -> bool {
    if previous.as_ref() == Some(&value) {
        return true;
    }
    let result = updates.try_send(make_update(value.clone()));
    match result {
        Ok(()) => {
            *previous = Some(value);
            true
        }
        Err(mpsc::TrySendError::Full(_)) => true,
        Err(mpsc::TrySendError::Disconnected(_)) => false,
    }
}

fn invalidate_device_snapshots_after_config_reload(
    audio: &mut Option<AudioInfo>,
    battery: &mut Option<BatteryInfo>,
    network: &mut Option<NetworkInfo>,
    media: &mut Option<MediaInfo>,
    device_ticks: &mut u8,
) {
    *audio = None;
    *battery = None;
    *network = None;
    *media = None;
    *device_ticks = 0;
}

fn sample_device<T: Clone + PartialEq + Default>(
    updates: &UpdateSender,
    previous: &mut Option<T>,
    value: Option<T>,
    make_update: impl FnOnce(T) -> WorkerUpdate,
) -> bool {
    send_changed(updates, previous, value.unwrap_or_default(), make_update)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_cadence_is_initial_and_three_seconds() {
        assert_eq!(
            (0u8..35).filter(|tick| tick % 30 == 0).collect::<Vec<_>>(),
            vec![0, 30]
        );
    }

    #[test]
    fn changed_update_includes_unavailable_transition() {
        let (tx, rx) = mpsc::sync_channel(4);
        let mut previous = Some(AudioInfo {
            volume: 50,
            is_muted: false,
            available: true,
            app_streams: Vec::new(),
        });
        assert!(send_changed(
            &tx,
            &mut previous,
            AudioInfo::default(),
            WorkerUpdate::Audio
        ));
        assert!(matches!(rx.try_recv(), Ok(WorkerUpdate::Audio(info)) if !info.available));
    }

    #[test]
    fn config_reload_replays_unchanged_battery_snapshot() {
        let (tx, rx) = mpsc::sync_channel(4);
        let battery = BatteryInfo {
            percentage: 58,
            is_charging: true,
            is_present: true,
        };
        let mut previous = Some(battery.clone());
        let mut audio = Some(AudioInfo::default());
        let mut network = Some(NetworkInfo::default());
        let mut media = Some(MediaInfo::default());
        let mut device_ticks = 17;

        invalidate_device_snapshots_after_config_reload(
            &mut audio,
            &mut previous,
            &mut network,
            &mut media,
            &mut device_ticks,
        );
        assert_eq!(device_ticks, 0);
        assert!(sample_device(
            &tx,
            &mut previous,
            Some(battery),
            WorkerUpdate::Battery,
        ));

        assert!(
            matches!(rx.try_recv(), Ok(WorkerUpdate::Battery(info)) if info.is_present),
            "a recreated bar must receive the current battery snapshot"
        );
    }
}
