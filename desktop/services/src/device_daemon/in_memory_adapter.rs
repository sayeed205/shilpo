use crate::device_protocol::{
    AudioAction, BrightnessAction, CaffeineAction, DeviceCommand, DeviceDomain, DomainLifecycle,
    DomainPayload, DomainState, NightLightAction,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub trait DeviceAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute_command(
        &self,
        command: DeviceCommand,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DomainState, String>> + Send + 'static>,
    >;
    fn get_domain_state(&self, domain: DeviceDomain) -> DomainState;
}

#[derive(Clone)]
pub struct InMemoryDeviceAdapter {
    states: Arc<Mutex<HashMap<DeviceDomain, DomainState>>>,
    pub forced_delay: Arc<Mutex<Option<Duration>>>,
    pub forced_error: Arc<Mutex<Option<String>>>,
}

impl InMemoryDeviceAdapter {
    pub fn new() -> Self {
        let mut states = HashMap::new();
        for domain in [
            DeviceDomain::Audio,
            DeviceDomain::Bluetooth,
            DeviceDomain::Brightness,
            DeviceDomain::Network,
            DeviceDomain::NightLight,
            DeviceDomain::PowerProfile,
            DeviceDomain::Media,
            DeviceDomain::Battery,
            DeviceDomain::Caffeine,
        ] {
            states.insert(
                domain,
                DomainState {
                    domain,
                    version: crate::device_protocol::DomainVersion::new(1, 1),
                    lifecycle: DomainLifecycle::Ready,
                    payload: match domain {
                        DeviceDomain::Audio => DomainPayload::Audio(crate::device_protocol::AudioPayload {
                            volume: 50,
                            ..Default::default()
                        }),
                        DeviceDomain::Brightness => {
                            DomainPayload::Brightness(crate::device_protocol::BrightnessPayload {
                                percentage: 70,
                                ..Default::default()
                            })
                        }
                        DeviceDomain::NightLight => {
                            DomainPayload::NightLight(crate::device_protocol::NightLightPayload {
                                temperature: 4000,
                                ..Default::default()
                            })
                        }
                        DeviceDomain::Caffeine => {
                            DomainPayload::Caffeine(crate::device_protocol::CaffeinePayload::default())
                        }
                        DeviceDomain::Bluetooth => {
                            DomainPayload::Bluetooth(crate::device_protocol::BluetoothPayload::default())
                        }
                        DeviceDomain::Network => {
                            DomainPayload::Network(crate::device_protocol::NetworkPayload::default())
                        }
                        DeviceDomain::PowerProfile => {
                            DomainPayload::PowerProfile(crate::device_protocol::PowerProfilePayload::default())
                        }
                        DeviceDomain::Media => DomainPayload::Media(crate::device_protocol::MediaPayload::default()),
                        DeviceDomain::Battery => {
                            DomainPayload::Battery(crate::device_protocol::BatteryPayload::default())
                        }
                    },
                    error: None,
                },
            );
        }

        Self {
            states: Arc::new(Mutex::new(states)),
            forced_delay: Arc::new(Mutex::new(None)),
            forced_error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_forced_delay(&self, delay: Option<Duration>) {
        *self.forced_delay.lock().unwrap() = delay;
    }

    pub fn set_forced_error(&self, error: Option<String>) {
        *self.forced_error.lock().unwrap() = error;
    }
}

impl Default for InMemoryDeviceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceAdapter for InMemoryDeviceAdapter {
    fn name(&self) -> &'static str {
        "in-memory"
    }

    fn get_domain_state(&self, domain: DeviceDomain) -> DomainState {
        self.states.lock().unwrap().get(&domain).cloned().unwrap()
    }

    fn execute_command(
        &self,
        command: DeviceCommand,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DomainState, String>> + Send + 'static>,
    > {
        let forced_delay = self.forced_delay.clone();
        let forced_error = self.forced_error.clone();
        let states = self.states.clone();

        Box::pin(async move {
            let delay = *forced_delay.lock().unwrap();
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }

            let forced_err = forced_error.lock().unwrap().clone();
            if let Some(err) = forced_err {
                return Err(err);
            }

            let domain = command.domain();
            let mut states_guard = states.lock().unwrap();
            let current = states_guard.get_mut(&domain).ok_or("Unknown domain")?;

            current.version.revision += 1;

            match command {
                DeviceCommand::Audio(action) => match action {
                    AudioAction::SetVolume(v) => {
                        if let DomainPayload::Audio(payload) = &mut current.payload {
                            payload.volume = v;
                        }
                    }
                    AudioAction::SetMuted(m) => {
                        if let DomainPayload::Audio(payload) = &mut current.payload {
                            payload.is_muted = m;
                        }
                    }
                    AudioAction::ToggleMute => {
                        if let DomainPayload::Audio(payload) = &mut current.payload {
                            payload.is_muted = !payload.is_muted;
                        }
                    }
                },
                DeviceCommand::Brightness(action) => match action {
                    BrightnessAction::SetBrightness(b) => {
                        if let DomainPayload::Brightness(payload) = &mut current.payload {
                            payload.percentage = b;
                        }
                    }
                    BrightnessAction::SetDisplay { percentage, .. } => {
                        if let DomainPayload::Brightness(payload) = &mut current.payload {
                            payload.percentage = percentage;
                        }
                    }
                    BrightnessAction::StepUp => {
                        if let DomainPayload::Brightness(payload) = &mut current.payload {
                            payload.percentage = (payload.percentage + 10).min(100);
                        }
                    }
                    BrightnessAction::StepDown => {
                        if let DomainPayload::Brightness(payload) = &mut current.payload {
                            payload.percentage = payload.percentage.saturating_sub(10);
                        }
                    }
                },
                DeviceCommand::NightLight(action) => match action {
                    NightLightAction::SetEnabled(e) => {
                        if let DomainPayload::NightLight(payload) = &mut current.payload {
                            payload.enabled = e;
                        }
                    }
                    NightLightAction::ToggleEnabled => {
                        if let DomainPayload::NightLight(payload) = &mut current.payload {
                            payload.enabled = !payload.enabled;
                        }
                    }
                    NightLightAction::SetTemperature(t) => {
                        if let DomainPayload::NightLight(payload) = &mut current.payload {
                            payload.temperature = t;
                        }
                    }
                },
                DeviceCommand::Caffeine(action) => match action {
                    CaffeineAction::SetEnabled(e) => {
                        if let DomainPayload::Caffeine(payload) = &mut current.payload {
                            payload.enabled = e;
                        }
                    }
                    CaffeineAction::Toggle => {
                        if let DomainPayload::Caffeine(payload) = &mut current.payload {
                            payload.enabled = !payload.enabled;
                        }
                    }
                },
                _ => {}
            }

            Ok(current.clone())
        })
    }
}
