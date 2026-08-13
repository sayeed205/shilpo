use std::{fs::File, io::Write, os::fd::AsFd};

use anyhow::{Context, Result, anyhow};
use rustix::fs::{MemfdFlags, memfd_create};
use tracing::{debug, error, info, warn};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    protocol::{
        wl_output::{self, WlOutput},
        wl_registry::{self, WlRegistry},
    },
};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1},
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};

use super::color::{generate_gamma_ramp, kelvin_to_rgb};

#[derive(Debug)]
struct OutputGammaInfo {
    output: WlOutput,
    gamma_control: Option<ZwlrGammaControlV1>,
    gamma_size: u32,
}

pub struct WlrGammaState {
    manager: Option<ZwlrGammaControlManagerV1>,
    outputs: Vec<OutputGammaInfo>,
    failed: bool,
}

impl WlrGammaState {
    fn new() -> Self {
        Self {
            manager: None,
            outputs: Vec::new(),
            failed: false,
        }
    }
}

impl Dispatch<WlRegistry, ()> for WlrGammaState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "zwlr_gamma_control_manager_v1" => {
                    let manager = registry.bind::<ZwlrGammaControlManagerV1, _, _>(
                        name,
                        version.min(1),
                        qh,
                        (),
                    );
                    state.manager = Some(manager);
                }
                "wl_output" => {
                    let output = registry.bind::<WlOutput, _, _>(name, version.min(4), qh, ());
                    state.outputs.push(OutputGammaInfo {
                        output,
                        gamma_control: None,
                        gamma_size: 0,
                    });
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, ()> for WlrGammaState {
    fn event(
        _state: &mut Self,
        _proxy: &WlOutput,
        _event: wl_output::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, ()> for WlrGammaState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrGammaControlManagerV1,
        _event: zwlr_gamma_control_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrGammaControlV1, usize> for WlrGammaState {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrGammaControlV1,
        event: zwlr_gamma_control_v1::Event,
        data: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let output_idx = *data;
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => {
                if let Some(out_info) = state.outputs.get_mut(output_idx) {
                    out_info.gamma_size = size;
                    debug!("Output {} gamma size reported: {}", output_idx, size);
                }
            }
            zwlr_gamma_control_v1::Event::Failed => {
                state.failed = true;
                error!(
                    "ZwlrGammaControlV1 reported failure for output {}",
                    output_idx
                );
            }
            _ => {}
        }
    }
}

pub struct WlrGammaBackend {
    conn: Connection,
    event_queue: EventQueue<WlrGammaState>,
    state: WlrGammaState,
    _qh: QueueHandle<WlrGammaState>,
}

impl WlrGammaBackend {
    pub fn try_init() -> Result<Self> {
        let conn = Connection::connect_to_env().context("Failed to connect to Wayland display")?;
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();
        let display = conn.display();

        let mut state = WlrGammaState::new();
        display.get_registry(&qh, ());

        // Initial roundtrip to populate registry globals
        event_queue
            .roundtrip(&mut state)
            .context("Failed initial Wayland roundtrip")?;

        let manager = state.manager.clone().ok_or_else(|| {
            anyhow!("zwlr_gamma_control_manager_v1 global protocol not advertised")
        })?;

        // Bind gamma control for all discovered outputs
        for idx in 0..state.outputs.len() {
            let output = &state.outputs[idx].output;
            let gamma_ctrl = manager.get_gamma_control(output, &qh, idx);
            state.outputs[idx].gamma_control = Some(gamma_ctrl);
        }

        // Second roundtrip to receive gamma_size events
        event_queue
            .roundtrip(&mut state)
            .context("Failed gamma control roundtrip")?;

        if state.failed {
            return Err(anyhow!("Gamma control failed during initialization"));
        }

        info!("Successfully initialized native WLR Wayland gamma control backend");

        Ok(Self {
            conn,
            event_queue,
            state,
            _qh: qh,
        })
    }

    pub fn apply(&mut self, active: bool, temperature_kelvin: u32) -> Result<()> {
        let (r, g, b) = if active {
            kelvin_to_rgb(temperature_kelvin)
        } else {
            (1.0, 1.0, 1.0)
        };

        for (idx, out_info) in self.state.outputs.iter().enumerate() {
            if out_info.gamma_size == 0 {
                warn!("Skipping output {} with 0 gamma size", idx);
                continue;
            }

            if let Some(ref gamma_control) = out_info.gamma_control {
                let ramp_bytes = generate_gamma_ramp(out_info.gamma_size as usize, r, g, b);

                let memfd_name = c"shilpo-gamma";
                let fd = memfd_create(memfd_name, MemfdFlags::CLOEXEC)
                    .context("Failed to create memfd for gamma ramp")?;

                let mut file = File::from(fd);
                file.write_all(&ramp_bytes)
                    .context("Failed to write gamma ramp to memfd")?;

                gamma_control.set_gamma(file.as_fd());
            }
        }

        self.conn
            .flush()
            .context("Failed to flush Wayland commands")?;
        self.event_queue.dispatch_pending(&mut self.state)?;

        Ok(())
    }
}
