use crate::{
    recorder::transform::transpose_if_transform_transposed,
    types::{RecordableOutput, RecordableWindow, RecordingSource, RecordingSourceCatalog},
};
use ffmpeg::Rational;
use std::collections::HashMap;
use wayland_backend::client::ObjectId;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_output::{self, Transform, WlOutput},
        wl_registry::WlRegistry,
    },
};

#[derive(Debug)]
pub(super) struct WorkerOutputProbe {
    pub global_name: u32,
    pub name: Option<String>,
    pub loc: Option<(i32, i32)>,
    pub size_pixels: Option<(i32, i32)>,
    pub refresh: Option<Rational>,
    pub output: WlOutput,
    pub has_recvd_done: bool,
    pub transform: Option<Transform>,
}

impl WorkerOutputProbe {
    pub fn complete(&self) -> Option<WorkerOutput> {
        if let (Some(name), Some(loc), Some(size_pixels), Some(refresh)) =
            (&self.name, &self.loc, &self.size_pixels, &self.refresh)
        {
            Some(WorkerOutput {
                loc: *loc,
                name: name.clone(),
                refresh: *refresh,
                size_pixels: *size_pixels,
                output: self.output.clone(),
                transform: self.transform.unwrap_or(Transform::Normal),
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct WorkerOutput {
    pub name: String,
    pub loc: (i32, i32),
    pub size_pixels: (i32, i32),
    pub refresh: Rational,
    pub output: WlOutput,
    pub transform: Transform,
}

impl WorkerOutput {
    pub fn size_screen_space(&self) -> (i32, i32) {
        transpose_if_transform_transposed(self.size_pixels, self.transform)
    }
}

pub(super) struct WorkerToplevelProbe {
    pub handle: ExtForeignToplevelHandleV1,
    pub identifier: String,
    pub title: String,
    pub app_id: String,
}

pub(super) fn select_output<'a>(
    outputs: impl IntoIterator<Item = &'a Option<WorkerOutput>>,
    source: &RecordingSource,
) -> Result<Option<WorkerOutput>, String> {
    let enabled: Vec<_> = outputs.into_iter().flatten().cloned().collect();
    if enabled.is_empty() {
        return Err("no usable outputs found on the compositor".into());
    }
    match source {
        RecordingSource::Output(name) if name.is_empty() || name == "primary" => Ok(enabled
            .iter()
            .find(|output| output.loc == (0, 0))
            .or_else(|| enabled.first())
            .cloned()),
        RecordingSource::Output(name) => enabled
            .into_iter()
            .find(|output| &output.name == name)
            .map(Some)
            .ok_or_else(|| format!("output {name} not found")),
        RecordingSource::Window { .. } => Ok(None),
    }
}

pub(super) fn select_toplevel<'a>(
    toplevels: impl IntoIterator<Item = &'a WorkerToplevelProbe>,
    source: &RecordingSource,
) -> Option<ExtForeignToplevelHandleV1> {
    let RecordingSource::Window { identifier, .. } = source else {
        return None;
    };
    toplevels
        .into_iter()
        .find(|toplevel| toplevel.identifier == *identifier)
        .map(|toplevel| toplevel.handle.clone())
}
use wayland_protocols::{
    ext::{
        foreign_toplevel_list::v1::client::{
            ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
            ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
        },
        image_capture_source::v1::client::ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
        image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
    },
    xdg::xdg_output::zv1::client::{
        zxdg_output_manager_v1::ZxdgOutputManagerV1,
        zxdg_output_v1::{self, ZxdgOutputV1},
    },
};

#[derive(Default)]
struct OutputProbe {
    name: Option<String>,
    make: Option<String>,
    model: Option<String>,
    logical_size: Option<(i32, i32)>,
}

#[derive(Default)]
struct WindowProbe {
    identifier: Option<String>,
    app_id: Option<String>,
    title: Option<String>,
}

#[derive(Default)]
struct SourceDiscovery {
    outputs: HashMap<ObjectId, OutputProbe>,
    windows: HashMap<ObjectId, WindowProbe>,
}

/// Discover recordable outputs and uniquely identified windows through the
/// same protocol family used by the native worker.
pub fn discover() -> Result<RecordingSourceCatalog, String> {
    let connection = Connection::connect_to_env()
        .map_err(|error| format!("could not connect to the Wayland compositor: {error}"))?;
    let (globals, mut queue) = registry_queue_init::<SourceDiscovery>(&connection)
        .map_err(|error| format!("could not initialize recording source discovery: {error}"))?;
    let qh = queue.handle();
    let output_manager: ZxdgOutputManagerV1 = globals
        .bind(&qh, 1..=ZxdgOutputManagerV1::interface().version, ())
        .map_err(|_| {
            "compositor does not support zxdg-output-manager-v1; output discovery is unavailable"
                .to_string()
        })?;

    let mut state = SourceDiscovery::default();
    let registry = globals.registry();
    for global in globals.contents().clone_list() {
        if global.interface == WlOutput::interface().name {
            let output: WlOutput = registry.bind(global.name, global.version, &qh, ());
            let output_id = output.id();
            output_manager.get_xdg_output(&output, &qh, output_id.clone());
            state.outputs.insert(output_id, OutputProbe::default());
        }
    }

    let advertised: std::collections::HashSet<_> = globals
        .contents()
        .clone_list()
        .into_iter()
        .map(|global| global.interface)
        .collect();
    let window_capture_available = advertised.contains(ExtForeignToplevelListV1::interface().name)
        && advertised.contains(ExtForeignToplevelImageCaptureSourceManagerV1::interface().name)
        && advertised.contains(ExtImageCopyCaptureManagerV1::interface().name);

    if window_capture_available {
        let toplevel_list: ExtForeignToplevelListV1 = globals
            .bind(&qh, 1..=ExtForeignToplevelListV1::interface().version, ())
            .map_err(|_| "window recording protocol disappeared during source discovery")?;
        // Keep the proxy alive until initial properties are dispatched.
        let _toplevel_list = toplevel_list;
        queue
            .roundtrip(&mut state)
            .map_err(|error| format!("could not discover recording sources: {error}"))?;
        queue
            .roundtrip(&mut state)
            .map_err(|error| format!("could not read recording source metadata: {error}"))?;
    } else {
        queue
            .roundtrip(&mut state)
            .map_err(|error| format!("could not discover recording outputs: {error}"))?;
    }

    let mut outputs: Vec<_> = state
        .outputs
        .into_values()
        .filter_map(|output| {
            Some(RecordableOutput {
                name: output.name?,
                make: output.make.filter(|value| !value.is_empty()),
                model: output.model.filter(|value| !value.is_empty()),
                logical_size: output.logical_size?,
            })
        })
        .collect();
    outputs.sort_by(|a, b| a.name.cmp(&b.name));

    let mut windows: Vec<_> = state
        .windows
        .into_values()
        .filter_map(|window| {
            Some(RecordableWindow {
                identifier: window.identifier.filter(|value| !value.is_empty())?,
                app_id: window.app_id.filter(|value| !value.is_empty())?,
                title: window.title.filter(|value| !value.is_empty())?,
            })
        })
        .collect();
    windows.sort_by(|a, b| a.title.cmp(&b.title).then(a.identifier.cmp(&b.identifier)));

    Ok(RecordingSourceCatalog { outputs, windows })
}

impl Dispatch<WlRegistry, GlobalListContents> for SourceDiscovery {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlOutput, ()> for SourceDiscovery {
    fn event(
        state: &mut Self,
        proxy: &WlOutput,
        event: <WlOutput as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Geometry { make, model, .. } = event
            && let Some(output) = state.outputs.get_mut(&proxy.id())
        {
            output.make = Some(make);
            output.model = Some(model);
        }
    }
}

impl Dispatch<ZxdgOutputV1, ObjectId> for SourceDiscovery {
    fn event(
        state: &mut Self,
        _proxy: &ZxdgOutputV1,
        event: <ZxdgOutputV1 as Proxy>::Event,
        output_id: &ObjectId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(output) = state.outputs.get_mut(output_id) else {
            return;
        };
        match event {
            zxdg_output_v1::Event::Name { name } => output.name = Some(name),
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                output.logical_size = Some((width, height));
            }
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for SourceDiscovery {
    fn event(
        _state: &mut Self,
        _proxy: &ZxdgOutputManagerV1,
        _event: <ZxdgOutputManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for SourceDiscovery {
    fn event(
        state: &mut Self,
        _proxy: &ExtForeignToplevelListV1,
        event: <ExtForeignToplevelListV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            state.windows.insert(toplevel.id(), WindowProbe::default());
        }
    }
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for SourceDiscovery {
    fn event(
        state: &mut Self,
        proxy: &ExtForeignToplevelHandleV1,
        event: <ExtForeignToplevelHandleV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(window) = state.windows.get_mut(&proxy.id()) else {
            return;
        };
        match event {
            ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                window.identifier = Some(identifier);
            }
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                window.app_id = Some(app_id);
            }
            ext_foreign_toplevel_handle_v1::Event::Title { title } => {
                window.title = Some(title);
            }
            ext_foreign_toplevel_handle_v1::Event::Closed => {
                state.windows.remove(&proxy.id());
            }
            _ => {}
        }
    }
}
