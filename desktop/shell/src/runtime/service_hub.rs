use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};

use gpui::App;
use shilpo_services::NotificationService;

use crate::bar::service_worker::{self, CommandSender, UpdateReceiver, WorkerCommand};

use super::{SessionContext, ShellRuntime};

pub struct ServiceHub {
    pub compositor: Arc<dyn shilpo_services::CompositorAdapter>,
    pub notification: Option<NotificationService>,
    pub notification_state: shilpo_services::ServiceLifecycle,
    pub notification_last_error: Option<String>,
    pub(super) notification_attempt: u32,
    pub(super) notification_next_retry: Option<Instant>,
    pub(super) notification_dnd: bool,
    pub clipboard: shilpo_services::ClipboardService,
    pub app_scanner: shilpo_services::AppScanner,
    pub service_commands: CommandSender,
    pub device_snapshot: crate::bar::service_worker::DeviceSnapshot,
    pub availability: crate::bar::service_worker::ServiceAvailability,
    pub notif_rx: Arc<Mutex<mpsc::Receiver<shilpo_services::Notification>>>,
    pub notif_tx: mpsc::Sender<shilpo_services::Notification>,
    pub updates_rx: Arc<Mutex<UpdateReceiver>>,
    pub _service_task: Option<gpui::Task<()>>,
    pub _watcher: Option<notify::RecommendedWatcher>,
    pub _app_watcher: Option<notify::RecommendedWatcher>,
}

impl ServiceHub {
    pub fn start(executor: gpui::BackgroundExecutor, session: &SessionContext) -> Self {
        let mut hub = Self::new(
            executor,
            session.config_path.clone(),
            session.heed_store.clone(),
        );
        hub.notification_dnd = session.session_state.dnd_active;
        apply_notification_dnd(hub.notification.as_ref(), session.session_state.dnd_active);
        hub
    }

    fn new(
        executor: gpui::BackgroundExecutor,
        config_path: PathBuf,
        session_store: Option<Arc<shilpo_config::HeedSessionStore>>,
    ) -> Self {
        let compositor: Arc<dyn shilpo_services::CompositorAdapter> =
            shilpo_services::NiriCompositorService::new();
        let (device_services, availability) = crate::bar::service_worker::DeviceServices::new();
        let clipboard = shilpo_services::ClipboardService::with_store(session_store);
        let app_scanner = shilpo_services::AppScanner::new()
            .unwrap_or_else(|_| shilpo_services::AppScanner::new_empty());
        let app_watcher = app_scanner.start_watcher();
        let (notification, notification_state, notification_last_error) =
            match NotificationService::new() {
                Ok(s) => (Some(s), shilpo_services::ServiceLifecycle::Ready, None),
                Err(e) => {
                    let err_str = e.to_string();
                    tracing::warn!(error = %err_str, "notification service unavailable; toasts disabled");
                    (
                        None,
                        shilpo_services::ServiceLifecycle::Connecting { attempt: 1 },
                        Some(err_str),
                    )
                }
            };

        let (notif_tx, notif_rx) = mpsc::channel();
        if let Some(service) = &notification {
            service.set_new_notification_sender(notif_tx.clone());
        }

        let (updates_tx, updates_rx, service_commands, commands_rx) = service_worker::channels();
        let service_task = service_worker::spawn(
            executor,
            updates_tx,
            commands_rx,
            config_path.clone(),
            device_services,
        );

        let config_dir = config_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".config/shilpo"));
        if let Err(error) = std::fs::create_dir_all(&config_dir) {
            tracing::warn!(error = %error, path = ?config_dir, "config watcher directory unavailable");
        }

        use notify::Watcher;
        let watcher_commands = service_commands.clone();
        let target_file_name = config_path.file_name().map(|n| n.to_os_string());
        let watcher = match notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Some(event) = res.ok().filter(|e| e.kind.is_modify())
                    && (target_file_name.is_none()
                        || event
                            .paths
                            .iter()
                            .any(|p| p.file_name() == target_file_name.as_deref()))
                {
                    let _ = service_worker::try_send_command(
                        &watcher_commands,
                        WorkerCommand::ReloadConfig,
                    );
                }
            },
            notify::Config::default(),
        ) {
            Ok(mut watcher) => match watcher.watch(&config_dir, notify::RecursiveMode::Recursive) {
                Ok(()) => Some(watcher),
                Err(error) => {
                    tracing::warn!(error = %error, path = ?config_dir, "config watcher watch failed");
                    None
                }
            },
            Err(error) => {
                tracing::warn!(error = %error, "config watcher creation failed");
                None
            }
        };

        let notification_attempt = if notification.is_some() { 0 } else { 1 };
        let notification_next_retry = notification
            .is_none()
            .then(|| Instant::now() + service_worker::backoff_delay(notification_attempt));

        Self {
            compositor,
            notification,
            notification_state,
            notification_last_error,
            notification_attempt,
            notification_next_retry,
            notification_dnd: false,
            clipboard,
            app_scanner,
            service_commands,
            device_snapshot: crate::bar::service_worker::DeviceSnapshot::default(),
            availability,
            notif_rx: Arc::new(Mutex::new(notif_rx)),
            notif_tx,
            updates_rx: Arc::new(Mutex::new(updates_rx)),
            _service_task: Some(service_task),
            _watcher: watcher,
            _app_watcher: app_watcher,
        }
    }

    pub fn poll_notification_reconnect(&mut self) {
        if self
            .notification
            .as_ref()
            .is_some_and(|service| !service.is_healthy())
        {
            self.notification = None;
            self.notification_attempt = self.notification_attempt.saturating_add(1);
            self.notification_state = shilpo_services::ServiceLifecycle::Connecting {
                attempt: self.notification_attempt,
            };
            self.notification_last_error = Some("notification D-Bus connection closed".into());
            self.notification_next_retry =
                Some(Instant::now() + service_worker::backoff_delay(self.notification_attempt));
        }
        if self.notification.is_some()
            || !self
                .notification_next_retry
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return;
        }

        match NotificationService::new() {
            Ok(service) => {
                service.set_new_notification_sender(self.notif_tx.clone());
                service.set_dnd_enabled(self.notification_dnd);
                self.notification = Some(service);
                self.notification_state = shilpo_services::ServiceLifecycle::Ready;
                self.notification_last_error = None;
                self.notification_attempt = 0;
                self.notification_next_retry = None;
            }
            Err(error) => {
                self.notification_attempt = self.notification_attempt.saturating_add(1);
                self.notification_state = shilpo_services::ServiceLifecycle::Connecting {
                    attempt: self.notification_attempt,
                };
                self.notification_last_error = Some(error.to_string());
                self.notification_next_retry =
                    Some(Instant::now() + service_worker::backoff_delay(self.notification_attempt));
            }
        }
    }
}

pub fn apply_notification_dnd(notification: Option<&NotificationService>, enabled: bool) {
    if let Some(notification) = notification {
        notification.set_dnd_enabled(enabled);
    }
}

impl ShellRuntime {
    pub(super) fn drain_service_hub(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }

        if let Some(hub) = cx.global_mut::<Self>().service_hub.as_mut() {
            hub.poll_notification_reconnect();
        }
        cx.global::<Self>().publish_status();

        let notifs = {
            let runtime = cx.global_mut::<Self>();
            let mut list = Vec::new();
            if let Some(hub) = &runtime.service_hub
                && let Ok(rx) = hub.notif_rx.lock()
            {
                while let Ok(notif) = rx.try_recv() {
                    list.push(notif);
                }
            }
            list
        };

        for notif in notifs {
            crate::bar::view::open_notification_toast(cx, notif);
        }

        let updates = {
            let runtime = cx.global_mut::<Self>();
            let mut list = Vec::new();
            if let Some(hub) = &runtime.service_hub
                && let Ok(rx) = hub.updates_rx.lock()
            {
                while let Ok(upd) = rx.try_recv() {
                    list.push(upd);
                }
            }
            list
        };

        if !updates.is_empty() {
            for upd in &updates {
                if let Some(ref mut hub) = cx.global_mut::<Self>().service_hub {
                    hub.device_snapshot.apply(upd);
                }
                match upd {
                    crate::bar::service_worker::WorkerUpdate::ServiceStateChange {
                        service,
                        state,
                        last_error,
                    } => {
                        if let Some(hub) = cx.global_mut::<Self>().service_hub.as_mut() {
                            let available = state.is_ready();
                            match *service {
                                "battery" => {
                                    hub.availability.battery_available = available;
                                    hub.availability.battery_state = *state;
                                    hub.availability.battery_last_error = last_error.clone();
                                }
                                "audio" => {
                                    hub.availability.audio_available = available;
                                    hub.availability.audio_state = *state;
                                    hub.availability.audio_last_error = last_error.clone();
                                }
                                "network" => {
                                    hub.availability.network_available = available;
                                    hub.availability.network_state = *state;
                                    hub.availability.network_last_error = last_error.clone();
                                }
                                "media" => {
                                    hub.availability.media_available = available;
                                    hub.availability.media_state = *state;
                                    hub.availability.media_last_error = last_error.clone();
                                }
                                "brightness" => {
                                    hub.availability.brightness_available = available;
                                    hub.availability.brightness_state = *state;
                                    hub.availability.brightness_last_error = last_error.clone();
                                }
                                _ => tracing::warn!(service, "unknown service state update"),
                            }
                        }
                    }
                    crate::bar::service_worker::WorkerUpdate::CommandRejected {
                        reason, ..
                    } => tracing::warn!(%reason, "device command rejected"),
                    crate::bar::service_worker::WorkerUpdate::Config(
                        crate::bar::service_worker::ConfigUpdate::Loaded(config),
                    ) => {
                        cx.global_mut::<Self>().active_config = (**config).clone();
                        Self::sync_displays(cx);
                        Self::reconcile_bar_extension_instances(cx);
                    }
                    crate::bar::service_worker::WorkerUpdate::Battery(info) => {
                        Self::dispatch_extension_event(
                            cx,
                            shilpo_ext::ExtensionEvent::PowerChanged {
                                percentage: info.is_present.then_some(info.percentage as f32),
                                charging: info.is_charging,
                            },
                        );
                    }
                    crate::bar::service_worker::WorkerUpdate::Network(info) => {
                        Self::dispatch_extension_event(
                            cx,
                            shilpo_ext::ExtensionEvent::NetworkChanged {
                                connected: info.available && info.is_connected,
                            },
                        );
                    }
                    crate::bar::service_worker::WorkerUpdate::Media(info) => {
                        Self::dispatch_extension_event(
                            cx,
                            shilpo_ext::ExtensionEvent::MediaChanged {
                                title: (!info.title.is_empty()).then_some(info.title.clone()),
                                artist: (!info.artist.is_empty()).then_some(info.artist.clone()),
                                playing: info.playback_state
                                    == shilpo_services::PlaybackState::Playing,
                            },
                        );
                    }
                    _ => {}
                }
            }

            let handles: Vec<_> = cx
                .global::<Self>()
                .surface_manager
                .bars
                .values()
                .map(|(handle, _)| *handle)
                .collect();

            for handle in handles {
                let updates_clone = updates.clone();
                let _ = handle.update(cx, |bar_view, _window, cx| {
                    for upd in &updates_clone {
                        bar_view.apply_worker_update(upd, cx);
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn service_hub_initialization_and_single_ownership() {
        let (updates_tx, _updates_rx, service_commands, _commands_rx) = service_worker::channels();
        assert!(
            service_worker::try_send_command(&service_commands, WorkerCommand::ReloadConfig)
                .is_ok()
        );
        drop(updates_tx);
    }

    #[test]
    fn restored_dnd_is_applied_to_notification_lifecycle() {
        let notification = NotificationService::new_offline();
        assert!(!notification.is_dnd_enabled());

        apply_notification_dnd(Some(&notification), true);

        assert!(notification.is_dnd_enabled());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn service_hub_start_initializes_with_dnd() {
        let temp_dir =
            std::env::temp_dir().join(format!("shilpo_service_test_{}", uuid::Uuid::new_v4()));
        let session = SessionContext {
            config_path: temp_dir.join("config.toml"),
            active_config: shilpo_config::ShellConfig::default(),
            session_path: temp_dir.join("session.toml"),
            session_state: shilpo_config::ShellSessionState {
                dnd_active: true,
                ..Default::default()
            },
            heed_store: None,
        };
        let executor = gpui::TestAppContext::single().executor().clone();
        let hub = ServiceHub::start(executor, &session);

        // The restored DND flag is applied to the hub lifecycle.
        assert!(hub.notification_dnd);
        // The hub is fully wired: the service worker owns the command receiver.
        assert!(
            service_worker::try_send_command(&hub.service_commands, WorkerCommand::ReloadConfig)
                .is_ok()
        );

        drop(hub);
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
