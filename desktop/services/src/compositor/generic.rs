//! Generic Wayland Compositor Backend (Tier 1)
//!
//! Operates over standardized Wayland protocols:
//! - `ext-workspace-v1` for workspace discovery, active state, and workspace switching.
//! - `ext-foreign-toplevel-list-v1` for toplevel listing and title/app_id observation. This
//!   protocol is read-only: it has no `activate`/`close` requests, so it grants
//!   `WindowIdentity::Fuzzy` but never `can_focus_window`/`can_close_window`.
//! - `zwlr-foreign-toplevel-management-unstable-v1` for actual window activation and close, and
//!   as the toplevel-listing fallback where `ext-foreign-toplevel-list-v1` is absent.
//!
//! Capabilities are derived from the runtime-bound protocols and degrade closed when disconnected.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use tokio::sync::watch;
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    protocol::{
        wl_output::{self, WlOutput},
        wl_registry::{self, WlRegistry},
        wl_seat::{self, WlSeat},
    },
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1},
    ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1},
    ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

use super::{
    BrokerOptions, CommandCancellation, CompositorAdapter, CompositorCapabilities,
    CompositorCommand, CompositorCommandBroker, CompositorExtras, CompositorOutput,
    CompositorSnapshot, CompositorTarget, DomainLifecycle, DomainVersion, ExecutorAck,
    RejectionReason, StaleUpdateError, SupervisorState, WindowIdentity, WindowInfo, WorkspaceInfo,
    broker::StreamCancelHandle,
    supervision::{apply_tick, record_supervisor_failure, sleep_with_stop_flag},
};
use crate::domain::{DomainSupervisor, MonotonicTimeSource, TimeSource};

/// Tracks which standardized Wayland protocols bound successfully at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BoundProtocols {
    pub ext_workspace: bool,
    pub ext_foreign_toplevel: bool,
    pub wlr_foreign_toplevel: bool,
}

impl BoundProtocols {
    /// Returns true if either `ext-foreign-toplevel-list-v1` or `zwlr-foreign-toplevel-management-v1` bound.
    pub fn has_toplevel(&self) -> bool {
        self.ext_foreign_toplevel || self.wlr_foreign_toplevel
    }

    /// Derives capabilities from bound protocols and connection lifecycle state.
    ///
    /// Degrades closed: all capabilities are false unless `lifecycle == DomainLifecycle::Ready`.
    ///
    /// `ext-foreign-toplevel-list-v1` has no `activate`/`close` requests (only `stop` and
    /// `destroy`, which release the client-side proxy, not the compositor-side window) — it is
    /// read-only. Only `wlr-foreign-toplevel-management-v1` can actually focus or close a
    /// window, so `can_focus_window`/`can_close_window` must gate on `wlr_foreign_toplevel`
    /// specifically, not on `has_toplevel()`.
    pub fn capabilities(&self, lifecycle: DomainLifecycle) -> CompositorCapabilities {
        if lifecycle != DomainLifecycle::Ready {
            return CompositorCapabilities::default();
        }

        let window_identity = if self.has_toplevel() {
            WindowIdentity::Fuzzy
        } else {
            WindowIdentity::None
        };

        CompositorCapabilities {
            window_identity,
            can_create_workspace: false,
            can_move_window: false,
            can_focus_window: self.wlr_foreign_toplevel,
            can_focus_workspace: self.ext_workspace,
            can_close_window: self.wlr_foreign_toplevel,
        }
    }
}

impl super::supervision::CapabilityProvider for BoundProtocols {
    fn capabilities_for(&self, lifecycle: DomainLifecycle) -> CompositorCapabilities {
        self.capabilities(lifecycle)
    }
}

impl super::supervision::CapabilityProvider for Arc<Mutex<BoundProtocols>> {
    fn capabilities_for(&self, lifecycle: DomainLifecycle) -> CompositorCapabilities {
        self.lock().unwrap().capabilities(lifecycle)
    }
}

/// Shared thread-safe proxy handles used for executing compositor commands.
#[derive(Default)]
pub(crate) struct GenericHandles {
    pub(crate) connection: Option<Connection>,
    pub(crate) workspace_manager: Option<ExtWorkspaceManagerV1>,
    pub(crate) workspaces: HashMap<u64, ExtWorkspaceHandleV1>,
    pub(crate) ext_windows: HashMap<u64, ExtForeignToplevelHandleV1>,
    pub(crate) wlr_windows: HashMap<u64, ZwlrForeignToplevelHandleV1>,
    pub(crate) seat: Option<WlSeat>,
}

/// Generic Wayland protocol backend implementing `CompositorAdapter`.
pub struct GenericWaylandCompositorBackend {
    supervision: super::supervision::CompositorSupervision<Arc<Mutex<BoundProtocols>>>,
    time_source: Arc<dyn TimeSource>,
    stop_flag: Arc<AtomicBool>,
    bound_protocols: Arc<Mutex<BoundProtocols>>,
    _shared_handles: Arc<Mutex<GenericHandles>>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl GenericWaylandCompositorBackend {
    /// Non-failing constructor. Publishes `Unavailable` immediately and spawns the background listener.
    pub fn new() -> Arc<Self> {
        let initial = CompositorSnapshot {
            version: DomainVersion::ZERO,
            connection: DomainLifecycle::Unavailable,
            capabilities: CompositorCapabilities::default(),
            outputs: Vec::new(),
            workspaces: Vec::new(),
            windows: Vec::new(),
            focused_output: None,
            focused_workspace_id: None,
            focused_window_id: None,
            active_keyboard_layout: None,
            extras: CompositorExtras::None,
            last_error: None,
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let time_source: Arc<dyn TimeSource> = Arc::new(MonotonicTimeSource::new());
        let bound_protocols = Arc::new(Mutex::new(BoundProtocols::default()));
        let shared_handles = Arc::new(Mutex::new(GenericHandles::default()));

        let handles_for_exec = shared_handles.clone();
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(move |cmd, timeout, cancel, register| {
                execute_generic_command(&handles_for_exec, cmd, timeout, cancel, register)
            }),
        );

        let supervision = super::supervision::CompositorSupervision::new(
            initial,
            broker,
            bound_protocols.clone(),
        );

        let tx_clone = supervision.tx.clone();
        let stop_clone = stop_flag.clone();
        let supervisor_clone = supervision.supervisor.clone();
        let time_source_clone = time_source.clone();
        let broker_clone = supervision.broker.clone();
        let bound_protocols_clone = bound_protocols.clone();
        let shared_handles_clone = shared_handles.clone();

        let handle = thread::spawn(move || {
            run_generic_listener(
                tx_clone,
                stop_clone,
                supervisor_clone,
                time_source_clone,
                broker_clone,
                bound_protocols_clone,
                shared_handles_clone,
            );
        });

        Arc::new(Self {
            supervision,
            time_source,
            stop_flag,
            bound_protocols,
            _shared_handles: shared_handles,
            handle: Mutex::new(Some(handle)),
        })
    }

    /// Constructs an offline instance with injected clock, broker, and initial snapshot.
    pub fn new_offline_with(
        snapshot: CompositorSnapshot,
        bound_protocols: BoundProtocols,
        time_source: Arc<dyn TimeSource>,
        broker: Arc<CompositorCommandBroker>,
    ) -> Arc<Self> {
        let bound_protocols = Arc::new(Mutex::new(bound_protocols));
        let supervision = super::supervision::CompositorSupervision::new(
            snapshot,
            broker,
            bound_protocols.clone(),
        );
        let stop_flag = Arc::new(AtomicBool::new(true));
        let shared_handles = Arc::new(Mutex::new(GenericHandles::default()));

        Arc::new(Self {
            supervision,
            time_source,
            stop_flag,
            bound_protocols,
            _shared_handles: shared_handles,
            handle: Mutex::new(None),
        })
    }

    /// Constructs an offline instance for testing with a specified initial snapshot.
    pub fn new_offline(snapshot: CompositorSnapshot) -> Arc<Self> {
        let broker = CompositorCommandBroker::new(
            BrokerOptions::default(),
            Box::new(|_cmd, _timeout, _cancel, _register| Ok(ExecutorAck::Success)),
        );
        Self::new_offline_with(
            snapshot,
            BoundProtocols {
                ext_workspace: true,
                ext_foreign_toplevel: true,
                wlr_foreign_toplevel: false,
            },
            Arc::new(MonotonicTimeSource::new()),
            broker,
        )
    }

    pub fn supervisor_state(&self) -> SupervisorState {
        self.supervision.supervisor_state()
    }

    pub fn time_source(&self) -> &Arc<dyn TimeSource> {
        &self.time_source
    }

    pub fn bound_protocols(&self) -> BoundProtocols {
        *self.bound_protocols.lock().unwrap()
    }

    pub fn begin_start(&self) {
        self.supervision.begin_start();
    }

    pub fn mark_ready(&self, now_ms: u64) {
        self.supervision.mark_ready(now_ms);
    }

    pub fn report_owner_failure(&self, error: String, now_ms: u64) {
        self.supervision.report_owner_failure(error, now_ms);
    }

    pub fn tick(&self, now_ms: u64) {
        self.supervision.tick(now_ms);
    }

    pub fn update_snapshot(&self, snapshot: CompositorSnapshot) -> Result<(), StaleUpdateError> {
        self.supervision.update_snapshot(snapshot)
    }

    pub fn set_reconnecting_generation(&self, generation: u64) {
        self.supervision.set_reconnecting_generation(generation);
    }

    pub fn reset_quarantine(&self) {
        self.supervision.reset_quarantine();
    }
}

impl CompositorAdapter for GenericWaylandCompositorBackend {
    fn current(&self) -> Arc<CompositorSnapshot> {
        self.supervision.rx.borrow().clone()
    }

    fn subscribe(&self) -> watch::Receiver<Arc<CompositorSnapshot>> {
        self.supervision.rx.clone()
    }

    fn command_broker(&self) -> Arc<CompositorCommandBroker> {
        self.supervision.broker.clone()
    }
}

impl Drop for GenericWaylandCompositorBackend {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.handle.lock()
            && let Some(handle) = guard.take()
        {
            let _ = handle.join();
        }
    }
}

fn execute_generic_command(
    handles: &Arc<Mutex<GenericHandles>>,
    command: &CompositorCommand,
    _timeout: Duration,
    cancel: Arc<CommandCancellation>,
    _register_cancel: &dyn Fn(Arc<dyn StreamCancelHandle>),
) -> Result<ExecutorAck, RejectionReason> {
    if cancel.is_cancelled() {
        return Err(RejectionReason::Cancelled(
            cancel.reason().unwrap_or(crate::CancellationReason::User),
        ));
    }

    let guard = handles.lock().unwrap();

    match command {
        CompositorCommand::FocusWorkspace(id) => {
            if let Some(ws) = guard.workspaces.get(id) {
                ws.activate();
                if let Some(ref mgr) = guard.workspace_manager {
                    mgr.commit();
                }
                if let Some(ref conn) = guard.connection {
                    let _ = conn.flush();
                }
                Ok(ExecutorAck::Success)
            } else {
                Err(RejectionReason::InvalidTarget(CompositorTarget::Workspace(
                    *id,
                )))
            }
        }
        CompositorCommand::FocusWindow(id) => {
            if let Some(wlr_win) = guard.wlr_windows.get(id) {
                if let Some(ref seat) = guard.seat {
                    wlr_win.activate(seat);
                    if let Some(ref conn) = guard.connection {
                        let _ = conn.flush();
                    }
                    Ok(ExecutorAck::Success)
                } else {
                    Err(RejectionReason::Unavailable)
                }
            } else if guard.ext_windows.contains_key(id) {
                // `ext-foreign-toplevel-list-v1` has no `activate` request -- it is read-only.
                // A window only known via that protocol cannot actually be focused.
                Err(RejectionReason::Unsupported)
            } else {
                Err(RejectionReason::InvalidTarget(CompositorTarget::Window(
                    *id,
                )))
            }
        }
        CompositorCommand::CloseWindow(id) => {
            if let Some(wlr_win) = guard.wlr_windows.get(id) {
                wlr_win.close();
                if let Some(ref conn) = guard.connection {
                    let _ = conn.flush();
                }
                Ok(ExecutorAck::Success)
            } else if guard.ext_windows.contains_key(id) {
                // `ext-foreign-toplevel-list-v1` has no `close` request -- `destroy` only
                // releases the client-side proxy, it does not ask the compositor to close the
                // window. A window only known via that protocol cannot actually be closed.
                Err(RejectionReason::Unsupported)
            } else {
                Err(RejectionReason::InvalidTarget(CompositorTarget::Window(
                    *id,
                )))
            }
        }
        CompositorCommand::CreateWorkspace
        | CompositorCommand::FocusPreviousWindow
        | CompositorCommand::MoveWindowToWorkspace { .. } => Err(RejectionReason::Unsupported),
    }
}

// ---------------------------------------------------------------------------
// Wayland Protocol Dispatch State & Listener Loop
// ---------------------------------------------------------------------------

struct InternalWorkspace {
    id: u64,
    name: Option<String>,
    idx: u32,
    is_active: bool,
    is_focused: bool,
    is_urgent: bool,
    output_name: Option<String>,
}

struct InternalWindow {
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    workspace_id: Option<u64>,
    is_focused: bool,
    is_floating: bool,
    is_urgent: bool,
}

struct InternalOutput {
    name: String,
    make: Option<String>,
    model: Option<String>,
    logical_position: (i32, i32),
    logical_size: (u32, u32),
    scale: f64,
}

pub struct WaylandState {
    bound_protocols: BoundProtocols,
    workspace_manager: Option<ExtWorkspaceManagerV1>,
    ext_toplevel_list: Option<ExtForeignToplevelListV1>,
    wlr_toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
    seat: Option<WlSeat>,

    outputs: HashMap<WlOutput, InternalOutput>,
    workspaces: HashMap<ExtWorkspaceHandleV1, InternalWorkspace>,
    ext_windows: HashMap<ExtForeignToplevelHandleV1, InternalWindow>,
    wlr_windows: HashMap<ZwlrForeignToplevelHandleV1, InternalWindow>,

    next_workspace_id: u64,
    next_window_id: u64,

    tx: watch::Sender<Arc<CompositorSnapshot>>,
    broker: Arc<CompositorCommandBroker>,
    shared_handles: Arc<Mutex<GenericHandles>>,
    owner_generation: u64,
    revision: u64,
}

impl WaylandState {
    fn new(
        tx: watch::Sender<Arc<CompositorSnapshot>>,
        broker: Arc<CompositorCommandBroker>,
        shared_handles: Arc<Mutex<GenericHandles>>,
        owner_generation: u64,
    ) -> Self {
        Self {
            bound_protocols: BoundProtocols::default(),
            workspace_manager: None,
            ext_toplevel_list: None,
            wlr_toplevel_manager: None,
            seat: None,
            outputs: HashMap::new(),
            workspaces: HashMap::new(),
            ext_windows: HashMap::new(),
            wlr_windows: HashMap::new(),
            next_workspace_id: 1,
            next_window_id: 1,
            tx,
            broker,
            shared_handles,
            owner_generation,
            revision: 1,
        }
    }

    fn publish_snapshot(&mut self) {
        self.revision = self.revision.saturating_add(1);

        let mut workspaces_vec: Vec<WorkspaceInfo> = self
            .workspaces
            .values()
            .map(|w| WorkspaceInfo {
                id: w.id,
                name: w.name.clone(),
                idx: w.idx,
                is_active: w.is_active,
                is_focused: w.is_focused,
                is_urgent: w.is_urgent,
                output_name: w.output_name.clone(),
                active_window_id: None,
            })
            .collect();
        workspaces_vec.sort_by_key(|w| (w.idx, w.id));

        let mut windows_vec: Vec<WindowInfo> = Vec::new();
        for w in self.ext_windows.values() {
            windows_vec.push(WindowInfo {
                id: w.id,
                title: w.title.clone(),
                app_id: w.app_id.clone(),
                workspace_id: w.workspace_id,
                is_focused: w.is_focused,
                is_floating: w.is_floating,
                is_urgent: w.is_urgent,
                layout_x: None,
                layout_y: None,
            });
        }
        for w in self.wlr_windows.values() {
            windows_vec.push(WindowInfo {
                id: w.id,
                title: w.title.clone(),
                app_id: w.app_id.clone(),
                workspace_id: w.workspace_id,
                is_focused: w.is_focused,
                is_floating: w.is_floating,
                is_urgent: w.is_urgent,
                layout_x: None,
                layout_y: None,
            });
        }
        windows_vec.sort_by_key(|w| w.id);

        let focused_workspace_id = workspaces_vec.iter().find(|w| w.is_focused).map(|w| w.id);
        let focused_window_id = windows_vec.iter().find(|w| w.is_focused).map(|w| w.id);

        let outputs_vec: Vec<CompositorOutput> = self
            .outputs
            .values()
            .map(|o| CompositorOutput {
                name: o.name.clone(),
                make: o.make.clone(),
                model: o.model.clone(),
                logical_position: o.logical_position,
                logical_size: o.logical_size,
                scale: o.scale,
            })
            .collect();

        // Sync shared handles for executor
        {
            let mut handles = self.shared_handles.lock().unwrap();
            handles.workspace_manager = self.workspace_manager.clone();
            handles.seat = self.seat.clone();
            handles.workspaces.clear();
            for (proxy, internal) in &self.workspaces {
                handles.workspaces.insert(internal.id, proxy.clone());
            }
            handles.ext_windows.clear();
            for (proxy, internal) in &self.ext_windows {
                handles.ext_windows.insert(internal.id, proxy.clone());
            }
            handles.wlr_windows.clear();
            for (proxy, internal) in &self.wlr_windows {
                handles.wlr_windows.insert(internal.id, proxy.clone());
            }
        }

        let snapshot = CompositorSnapshot {
            version: DomainVersion::new(self.owner_generation, self.revision),
            connection: DomainLifecycle::Ready,
            capabilities: self.bound_protocols.capabilities(DomainLifecycle::Ready),
            outputs: outputs_vec,
            workspaces: workspaces_vec,
            windows: windows_vec,
            focused_output: None,
            focused_workspace_id,
            focused_window_id,
            active_keyboard_layout: None,
            extras: CompositorExtras::None,
            last_error: None,
        };

        let snap_arc = Arc::new(snapshot);
        if self.broker.observe_snapshot(snap_arc.clone()).is_ok() {
            let _ = self.tx.send(snap_arc);
        }
    }
}

impl Dispatch<WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "ext_workspace_manager_v1" => {
                    let mgr =
                        registry.bind::<ExtWorkspaceManagerV1, _, _>(name, version.min(1), qh, ());
                    state.workspace_manager = Some(mgr);
                    state.bound_protocols.ext_workspace = true;
                }
                "ext_foreign_toplevel_list_v1" => {
                    let list = registry.bind::<ExtForeignToplevelListV1, _, _>(
                        name,
                        version.min(1),
                        qh,
                        (),
                    );
                    state.ext_toplevel_list = Some(list);
                    state.bound_protocols.ext_foreign_toplevel = true;
                }
                "zwlr_foreign_toplevel_manager_v1" => {
                    // Bind WLR foreign toplevel manager if ext-foreign-toplevel is absent or as fallback
                    let mgr = registry.bind::<ZwlrForeignToplevelManagerV1, _, _>(
                        name,
                        version.min(3),
                        qh,
                        (),
                    );
                    state.wlr_toplevel_manager = Some(mgr);
                    state.bound_protocols.wlr_foreign_toplevel = true;
                }
                "wl_seat" => {
                    let seat = registry.bind::<WlSeat, _, _>(name, version.min(7), qh, ());
                    state.seat = Some(seat);
                }
                "wl_output" => {
                    let output = registry.bind::<WlOutput, _, _>(name, version.min(4), qh, ());
                    state.outputs.insert(
                        output,
                        InternalOutput {
                            name: format!("output-{name}"),
                            make: None,
                            model: None,
                            logical_position: (0, 0),
                            logical_size: (1920, 1080),
                            scale: 1.0,
                        },
                    );
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlOutput, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &WlOutput,
        event: wl_output::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let Some(output_data) = state.outputs.get_mut(proxy) {
            match event {
                wl_output::Event::Name { name } => {
                    output_data.name = name;
                }
                wl_output::Event::Geometry {
                    make, model, x, y, ..
                } => {
                    output_data.make = Some(make);
                    output_data.model = Some(model);
                    output_data.logical_position = (x, y);
                }
                wl_output::Event::Scale { factor } => {
                    output_data.scale = factor as f64;
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ExtWorkspaceManagerV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtWorkspaceManagerV1,
        event: ext_workspace_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_workspace_manager_v1::Event::Done => {
                _state.publish_snapshot();
            }
            ext_workspace_manager_v1::Event::Finished => {}
            _ => {}
        }
    }
}

impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtWorkspaceGroupHandleV1,
        _event: ext_workspace_group_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let next_id = state.next_workspace_id;
        let internal = state.workspaces.entry(proxy.clone()).or_insert_with(|| {
            state.next_workspace_id = next_id.saturating_add(1);
            InternalWorkspace {
                id: next_id,
                name: None,
                idx: (next_id.saturating_sub(1)) as u32,
                is_active: false,
                is_focused: false,
                is_urgent: false,
                output_name: None,
            }
        });

        match event {
            ext_workspace_handle_v1::Event::Name { name } => {
                internal.name = Some(name);
            }
            ext_workspace_handle_v1::Event::Coordinates { coordinates } => {
                if coordinates.len() >= 4 {
                    let mut b = [0u8; 4];
                    b.copy_from_slice(&coordinates[0..4]);
                    internal.idx = u32::from_ne_bytes(b);
                }
            }
            ext_workspace_handle_v1::Event::State { state } => match state {
                wayland_client::WEnum::Value(ext_workspace_handle_v1::State::Active) => {
                    internal.is_active = true;
                    internal.is_focused = true;
                }
                wayland_client::WEnum::Value(ext_workspace_handle_v1::State::Urgent) => {
                    internal.is_urgent = true;
                }
                wayland_client::WEnum::Value(ext_workspace_handle_v1::State::Hidden) => {
                    internal.is_active = false;
                    internal.is_focused = false;
                }
                _ => {}
            },
            ext_workspace_handle_v1::Event::Id { id } => {
                if internal.name.is_none() {
                    internal.name = Some(id);
                }
            }
            ext_workspace_handle_v1::Event::Removed => {
                state.workspaces.remove(proxy);
                state.publish_snapshot();
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let _ = event;
    }
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let next_id = state.next_window_id;
        let internal = state.ext_windows.entry(proxy.clone()).or_insert_with(|| {
            state.next_window_id = next_id.saturating_add(1);
            InternalWindow {
                id: next_id,
                title: None,
                app_id: None,
                workspace_id: None,
                is_focused: false,
                is_floating: false,
                is_urgent: false,
            }
        });

        match event {
            ext_foreign_toplevel_handle_v1::Event::Title { title } => {
                internal.title = Some(title);
            }
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                internal.app_id = Some(app_id);
            }
            ext_foreign_toplevel_handle_v1::Event::Done => {
                state.publish_snapshot();
            }
            ext_foreign_toplevel_handle_v1::Event::Closed => {
                state.ext_windows.remove(proxy);
                state.publish_snapshot();
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let _ = event;
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let next_id = state.next_window_id;
        let internal = state.wlr_windows.entry(proxy.clone()).or_insert_with(|| {
            state.next_window_id = next_id.saturating_add(1);
            InternalWindow {
                id: next_id,
                title: None,
                app_id: None,
                workspace_id: None,
                is_focused: false,
                is_floating: false,
                is_urgent: false,
            }
        });

        match event {
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                internal.title = Some(title);
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                internal.app_id = Some(app_id);
            }
            zwlr_foreign_toplevel_handle_v1::Event::State { state: state_bytes } => {
                let mut is_activated = false;
                for chunk in state_bytes.chunks_exact(4) {
                    let mut b = [0u8; 4];
                    b.copy_from_slice(chunk);
                    let val = u32::from_ne_bytes(b);
                    if val == 2 {
                        // Activated
                        is_activated = true;
                    }
                }
                internal.is_focused = is_activated;
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                state.publish_snapshot();
            }
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                state.wlr_windows.remove(proxy);
                state.publish_snapshot();
            }
            _ => {}
        }
    }
}

fn run_generic_listener(
    tx: watch::Sender<Arc<CompositorSnapshot>>,
    stop_flag: Arc<AtomicBool>,
    supervisor: Arc<Mutex<DomainSupervisor>>,
    time_source: Arc<dyn TimeSource>,
    broker: Arc<CompositorCommandBroker>,
    bound_protocols: Arc<Mutex<BoundProtocols>>,
    shared_handles: Arc<Mutex<GenericHandles>>,
) {
    let mut owner_generation = 0u64;

    while !stop_flag.load(Ordering::Relaxed) {
        let now_ms = time_source.now_ms();

        apply_tick(&supervisor, now_ms);

        let state = supervisor.lock().unwrap().state();
        match state {
            SupervisorState::Quarantined => {
                sleep_with_stop_flag(Duration::from_millis(100), &stop_flag);
                continue;
            }
            SupervisorState::Backoff { retry_at_ms, .. } => {
                let remaining_ms = retry_at_ms.saturating_sub(now_ms);
                let sleep_duration = Duration::from_millis(remaining_ms.clamp(1, 100));
                sleep_with_stop_flag(sleep_duration, &stop_flag);
                continue;
            }
            SupervisorState::Starting | SupervisorState::Running => {}
            SupervisorState::Stopping | SupervisorState::Stopped => {
                break;
            }
        }

        owner_generation += 1;
        let mut revision = 0u64;
        {
            supervisor.lock().unwrap().mark_starting();
            tracing::info!(target: "shilpo_profile", lifecycle = "starting", "generic compositor supervisor transition");
        }
        broker.set_installed_generation(owner_generation);
        broker.record_restart();

        let previous = tx.borrow().clone();
        let mut connecting = (*previous).clone();
        revision = revision.saturating_add(1);
        connecting.version = DomainVersion::new(owner_generation, revision);
        connecting.connection = DomainLifecycle::Connecting;
        connecting.capabilities = CompositorCapabilities::default();
        connecting.last_error = None;
        let connecting = Arc::new(connecting);
        if broker.observe_snapshot(connecting.clone()).is_ok() {
            let _ = tx.send(connecting);
        }

        // Connect to Wayland server
        let conn = match Connection::connect_to_env() {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Failed to connect to Wayland environment: {e}");
                let now_ms = time_source.now_ms();
                record_supervisor_failure(
                    &supervisor,
                    &broker,
                    &tx,
                    owner_generation,
                    &mut revision,
                    err_msg,
                    now_ms,
                    &BoundProtocols::default(),
                );
                continue;
            }
        };

        shared_handles.lock().unwrap().connection = Some(conn.clone());

        let mut event_queue: EventQueue<WaylandState> = conn.new_event_queue();
        let qh = event_queue.handle();
        let display = conn.display();
        let _registry = display.get_registry(&qh, ());

        let mut wayland_state = WaylandState::new(
            tx.clone(),
            broker.clone(),
            shared_handles.clone(),
            owner_generation,
        );

        // Roundtrip 1: Discover globals
        if let Err(e) = event_queue.roundtrip(&mut wayland_state) {
            let err_msg = format!("Failed during registry discovery roundtrip: {e}");
            let now_ms = time_source.now_ms();
            record_supervisor_failure(
                &supervisor,
                &broker,
                &tx,
                owner_generation,
                &mut revision,
                err_msg,
                now_ms,
                &BoundProtocols::default(),
            );
            continue;
        }

        // Roundtrip 2: Initial events from bound managers
        if let Err(e) = event_queue.roundtrip(&mut wayland_state) {
            let err_msg = format!("Failed during initial protocol event roundtrip: {e}");
            let now_ms = time_source.now_ms();
            record_supervisor_failure(
                &supervisor,
                &broker,
                &tx,
                owner_generation,
                &mut revision,
                err_msg,
                now_ms,
                &BoundProtocols::default(),
            );
            continue;
        }

        let bounds = wayland_state.bound_protocols;
        *bound_protocols.lock().unwrap() = bounds;

        // Verify that at least one protocol is supported
        if !bounds.ext_workspace && !bounds.has_toplevel() {
            let err_msg =
                "Compositor supports neither ext-workspace-v1 nor foreign-toplevel protocols"
                    .to_string();
            let now_ms = time_source.now_ms();
            record_supervisor_failure(
                &supervisor,
                &broker,
                &tx,
                owner_generation,
                &mut revision,
                err_msg,
                now_ms,
                &bounds,
            );
            continue;
        }

        // Mark running
        {
            let now_ms = time_source.now_ms();
            supervisor.lock().unwrap().mark_running(now_ms);
            tracing::info!(
                target: "shilpo_profile",
                lifecycle = "ready",
                ext_workspace = bounds.ext_workspace,
                ext_foreign_toplevel = bounds.ext_foreign_toplevel,
                wlr_foreign_toplevel = bounds.wlr_foreign_toplevel,
                "generic compositor ready"
            );
        }

        // Publish initial ready snapshot
        wayland_state.publish_snapshot();

        // Run event dispatch loop
        while !stop_flag.load(Ordering::Relaxed) {
            if let Err(e) = event_queue.blocking_dispatch(&mut wayland_state) {
                let err_msg = format!("Wayland dispatch loop terminated: {e}");
                let now_ms = time_source.now_ms();
                record_supervisor_failure(
                    &supervisor,
                    &broker,
                    &tx,
                    owner_generation,
                    &mut revision,
                    err_msg,
                    now_ms,
                    &bounds,
                );
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bound_protocols_capability_matrix() {
        // 1. Workspace + ext-only toplevel listing (read-only: no activate/close requests)
        let ext_only = BoundProtocols {
            ext_workspace: true,
            ext_foreign_toplevel: true,
            wlr_foreign_toplevel: false,
        };
        let caps = ext_only.capabilities(DomainLifecycle::Ready);
        assert_eq!(caps.window_identity, WindowIdentity::Fuzzy);
        assert!(caps.can_focus_workspace);
        assert!(!caps.can_create_workspace);
        assert!(!caps.can_move_window);
        assert!(!caps.can_focus_window);
        assert!(!caps.can_close_window);

        // 2. Only workspace present
        let ws_only = BoundProtocols {
            ext_workspace: true,
            ext_foreign_toplevel: false,
            wlr_foreign_toplevel: false,
        };
        let caps_ws = ws_only.capabilities(DomainLifecycle::Ready);
        assert_eq!(caps_ws.window_identity, WindowIdentity::None);
        assert!(caps_ws.can_focus_workspace);
        assert!(!caps_ws.can_focus_window);
        assert!(!caps_ws.can_close_window);

        // 3. Only WLR toplevel present
        let wlr_only = BoundProtocols {
            ext_workspace: false,
            ext_foreign_toplevel: false,
            wlr_foreign_toplevel: true,
        };
        let caps_wlr = wlr_only.capabilities(DomainLifecycle::Ready);
        assert_eq!(caps_wlr.window_identity, WindowIdentity::Fuzzy);
        assert!(!caps_wlr.can_focus_workspace);
        assert!(caps_wlr.can_focus_window);
        assert!(caps_wlr.can_close_window);

        // 4. Neither present
        let none = BoundProtocols::default();
        let caps_none = none.capabilities(DomainLifecycle::Ready);
        assert_eq!(caps_none.window_identity, WindowIdentity::None);
        assert!(!caps_none.can_focus_workspace);
        assert!(!caps_none.can_focus_window);
        assert!(!caps_none.can_close_window);

        // 5. Both toplevel protocols present: wlr grants real control regardless of ext also
        // being bound.
        let both = BoundProtocols {
            ext_workspace: true,
            ext_foreign_toplevel: true,
            wlr_foreign_toplevel: true,
        };
        let caps_both = both.capabilities(DomainLifecycle::Ready);
        assert_eq!(caps_both.window_identity, WindowIdentity::Fuzzy);
        assert!(caps_both.can_focus_window);
        assert!(caps_both.can_close_window);
    }

    #[test]
    fn test_capabilities_degrade_closed_when_not_ready() {
        let both = BoundProtocols {
            ext_workspace: true,
            ext_foreign_toplevel: true,
            wlr_foreign_toplevel: true,
        };

        for non_ready in [
            DomainLifecycle::Unavailable,
            DomainLifecycle::Connecting,
            DomainLifecycle::Reconnecting,
            DomainLifecycle::Degraded,
        ] {
            let caps = both.capabilities(non_ready);
            assert_eq!(caps, CompositorCapabilities::default());
            assert_eq!(caps.window_identity, WindowIdentity::None);
        }
    }

    #[test]
    fn test_window_identity_is_never_exact() {
        let bounds = BoundProtocols {
            ext_workspace: true,
            ext_foreign_toplevel: true,
            wlr_foreign_toplevel: true,
        };
        let caps = bounds.capabilities(DomainLifecycle::Ready);
        assert_ne!(caps.window_identity, WindowIdentity::Exact);
        assert_eq!(caps.window_identity, WindowIdentity::Fuzzy);
    }

    #[test]
    fn test_offline_backend_rejection_for_unsupported_commands() {
        let snapshot = CompositorSnapshot {
            connection: DomainLifecycle::Ready,
            capabilities: BoundProtocols {
                ext_workspace: true,
                ext_foreign_toplevel: false,
                wlr_foreign_toplevel: false,
            }
            .capabilities(DomainLifecycle::Ready),
            ..Default::default()
        };

        let backend = GenericWaylandCompositorBackend::new_offline(snapshot);
        let broker = backend.command_broker();

        // Focus window must be rejected as unsupported because toplevel protocol was not bound
        let res = broker.submit(CompositorCommand::FocusWindow(10));
        assert!(matches!(
            res,
            Err(crate::CommandOutcome::Rejected {
                reason: RejectionReason::Unsupported
            })
        ));

        // Create workspace must be rejected as unsupported
        let res2 = broker.submit(CompositorCommand::CreateWorkspace);
        assert!(matches!(
            res2,
            Err(crate::CommandOutcome::Rejected {
                reason: RejectionReason::Unsupported
            })
        ));
    }
}
