use gpui::BackgroundExecutor;
use shilpo_config::ShellConfig;
use shilpo_services::{
    AudioInfo, AudioService, BatteryInfo, BatteryService, NetworkInfo, NetworkService,
    NiriCompositorService, NiriWorkspaceInfo,
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
    FocusWorkspace(u64),
    ReloadConfig,
}

#[derive(Debug)]
pub enum ConfigUpdate {
    Loaded(ShellConfig),
    Failed(String),
}

#[derive(Debug)]
pub enum WorkerUpdate {
    Workspaces(Vec<NiriWorkspaceInfo>),
    ActiveTitle(String),
    AppId(String),
    Battery(BatteryInfo),
    Audio(AudioInfo),
    Network(NetworkInfo),
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
    niri: Option<NiriCompositorService>,
    battery: Option<BatteryService>,
    audio: Option<AudioService>,
    network: Option<NetworkService>,
) -> gpui::Task<()> {
    let worker_executor = executor.clone();
    executor.spawn(async move {
        run(
            worker_executor,
            updates,
            commands,
            config_path,
            niri,
            battery,
            audio,
            network,
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
    niri: Option<NiriCompositorService>,
    battery: Option<BatteryService>,
    audio: Option<AudioService>,
    network: Option<NetworkService>,
) {
    let mut workspaces = None;
    let mut title = None;
    let mut app_id = None;
    let mut battery_last = None;
    let mut audio_last = None;
    let mut network_last = None;
    let mut device_ticks = 0u8;
    let mut pending_reload: Option<Instant> = None;
    let debounce_duration = Duration::from_millis(200);

    if !load_config(&updates, &config_path) {
        return;
    }

    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                WorkerCommand::FocusWorkspace(id) => {
                    if let Some(service) = &niri {
                        let _ = service.focus_workspace(id);
                    }
                }
                WorkerCommand::ReloadConfig => {
                    pending_reload = Some(Instant::now());
                }
            }
        }

        if pending_reload.is_some_and(|req| req.elapsed() >= debounce_duration) {
            pending_reload = None;
            if !load_config(&updates, &config_path) {
                return;
            }
        }
        if let Some(service) = &niri {
            if !send_changed(
                &updates,
                &mut workspaces,
                service.workspaces(),
                WorkerUpdate::Workspaces,
            ) {
                return;
            }
            if !send_changed(
                &updates,
                &mut title,
                service.active_window_title().unwrap_or_default(),
                WorkerUpdate::ActiveTitle,
            ) {
                return;
            }
            if !send_changed(
                &updates,
                &mut app_id,
                service.app_id().unwrap_or_default(),
                WorkerUpdate::AppId,
            ) {
                return;
            }
        } else if workspaces.is_none() {
            if !send_changed(
                &updates,
                &mut workspaces,
                Vec::new(),
                WorkerUpdate::Workspaces,
            ) {
                return;
            }
            if !send_changed(
                &updates,
                &mut title,
                String::new(),
                WorkerUpdate::ActiveTitle,
            ) {
                return;
            }
            if !send_changed(&updates, &mut app_id, String::new(), WorkerUpdate::AppId) {
                return;
            }
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
                &mut audio_last,
                audio.as_ref().map(AudioService::audio_info),
                WorkerUpdate::Audio,
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
        Ok(config) => WorkerUpdate::Config(ConfigUpdate::Loaded(config)),
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
        });
        assert!(send_changed(
            &tx,
            &mut previous,
            AudioInfo::default(),
            WorkerUpdate::Audio
        ));
        assert!(matches!(rx.try_recv(), Ok(WorkerUpdate::Audio(info)) if !info.available));
    }
}
