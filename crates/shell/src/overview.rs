use crate::runtime::ShellRuntime;
use gpui::{
    Animation, AnimationExt as _, App, AppContext, Context, DragMoveEvent, ElementId, Entity,
    FocusHandle, Focusable, Image, ImageFormat, ImageSource, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ObjectFit, ParentElement, Render, Role, ScrollWheelEvent,
    SharedString, StatefulInteractiveElement, Styled, StyledImage, Window, div, img,
    prelude::FluentBuilder, px,
};
use image::imageops::FilterType;
use shilpo_services::{Application, CompositorSnapshot, WindowInfo, WorkspaceInfo};
use shilpo_ui::{ActiveTheme, FocusTrapElement, StyledExt, animation::cubic_bezier};
use std::{
    collections::HashMap,
    io::Cursor,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

// ── Animation constants ────────────────────────────────────────────────────
const ENTER_DURATION: Duration = Duration::from_millis(250);
const EXIT_DURATION: Duration = Duration::from_millis(200);
const PREVIEW_WIDTH: f32 = 326.0;
const PREVIEW_HEIGHT: f32 = 183.0;
const APP_ICON_SIZE: f32 = 52.0;
const MAX_VISIBLE_WORKSPACES: usize = 3;
const PREVIEW_RADIUS: f32 = 20.0;
const INTER_WORKSPACE_RADIUS: f32 = 8.0;
const PREVIEW_GAP: f32 = 6.0;
const STAGE_HORIZONTAL_PADDING: f32 = 10.0;
const STAGE_VERTICAL_PADDING: f32 = 10.0;
const STAGE_BORDER_WIDTH: f32 = 1.0;
const WALLPAPER_PREVIEW_MAX_WIDTH: u32 = 960;
const WALLPAPER_PREVIEW_MAX_HEIGHT: u32 = 540;
const WALLPAPER_BLUR_SIGMA: f32 = 2.0;

fn filmstrip_height(workspace_count: usize) -> f32 {
    let visible_count = workspace_count.min(MAX_VISIBLE_WORKSPACES);
    if visible_count == 0 {
        return 0.0;
    }

    PREVIEW_HEIGHT * visible_count as f32 + PREVIEW_GAP * (visible_count - 1) as f32
}

fn adjacent_workspace(
    workspace_ids: &[u64],
    active_workspace_id: Option<u64>,
    forward: bool,
) -> Option<(u64, usize)> {
    if workspace_ids.len() < 2 {
        return None;
    }

    let current_index = active_workspace_id
        .and_then(|active_id| workspace_ids.iter().position(|&id| id == active_id))
        .unwrap_or(0);
    let target_index = if forward {
        (current_index + 1).min(workspace_ids.len() - 1)
    } else {
        current_index.saturating_sub(1)
    };

    (target_index != current_index).then(|| (workspace_ids[target_index], target_index))
}

fn workspace_view_start(
    current_start: usize,
    target_index: usize,
    workspace_count: usize,
) -> usize {
    let max_start = workspace_count.saturating_sub(MAX_VISIBLE_WORKSPACES);
    let current_start = current_start.min(max_start);
    if target_index < current_start {
        target_index
    } else if target_index >= current_start + MAX_VISIBLE_WORKSPACES {
        (target_index + 1 - MAX_VISIBLE_WORKSPACES).min(max_start)
    } else {
        current_start
    }
}

fn workspace_render_range(view_start: usize, workspace_count: usize) -> Range<usize> {
    let start = view_start.min(workspace_count.saturating_sub(MAX_VISIBLE_WORKSPACES));
    start..(start + MAX_VISIBLE_WORKSPACES).min(workspace_count)
}

fn blurred_wallpaper_preview(path: &Path) -> Option<Arc<Image>> {
    let wallpaper = image::open(path).ok()?.resize(
        WALLPAPER_PREVIEW_MAX_WIDTH,
        WALLPAPER_PREVIEW_MAX_HEIGHT,
        FilterType::Triangle,
    );
    let wallpaper = wallpaper.blur(WALLPAPER_BLUR_SIGMA);
    let mut bytes = Cursor::new(Vec::new());
    wallpaper
        .write_to(&mut bytes, image::ImageFormat::Png)
        .ok()?;
    Some(Arc::new(Image::from_bytes(
        ImageFormat::Png,
        bytes.into_inner(),
    )))
}

#[derive(Clone, Debug)]
struct DraggedOverviewWindow {
    window_id: u64,
    source_workspace_id: Option<u64>,
    title: SharedString,
    icon_path: Option<PathBuf>,
    region_index: usize,
    region_count: usize,
    top_radius: f32,
    bottom_radius: f32,
}

impl Render for DraggedOverviewWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let icon = app_icon(
            self.icon_path.clone(),
            self.title.as_ref(),
            px(56.),
            cx.theme().surface_container_highest,
            cx.theme().on_surface,
        );

        let region_count = self.region_count.max(1);
        let width = PREVIEW_WIDTH / region_count as f32;
        let inner_radius = px(5.);
        let top_radius = px(self.top_radius);
        let bottom_radius = px(self.bottom_radius);
        div()
            .w(px(width))
            .h(px(PREVIEW_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .rounded_tl(if self.region_index == 0 {
                top_radius
            } else {
                inner_radius
            })
            .rounded_bl(if self.region_index == 0 {
                bottom_radius
            } else {
                inner_radius
            })
            .rounded_tr(if self.region_index + 1 == region_count {
                top_radius
            } else {
                inner_radius
            })
            .rounded_br(if self.region_index + 1 == region_count {
                bottom_radius
            } else {
                inner_radius
            })
            .bg(cx.theme().surface_container_high.opacity(0.72))
            .border_1()
            .border_color(cx.theme().outline_variant.opacity(0.7))
            .shadow_lg()
            .child(icon)
    }
}

/// Phase of the overview lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewPhase {
    Entering,
    Visible,
    Exiting,
}

/// Why the overview is closing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewCloseReason {
    /// Escape, scrim click, toggle, or forced — restore prior focus.
    Cancel,
    /// Workspace or window card click — do not restore prior focus.
    Selection,
}

fn normalize_app_key(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".desktop")
        .to_ascii_lowercase()
}

fn build_app_icon_index(applications: Vec<Application>) -> HashMap<String, PathBuf> {
    let mut icons = HashMap::new();
    for app in applications {
        let Some(icon_path) = app.icon_path else {
            continue;
        };

        let mut aliases = vec![normalize_app_key(&app.name)];
        if let Some(icon) = app.icon.as_deref() {
            aliases.push(normalize_app_key(icon));
        }
        if let Some(stem) = app.desktop_file.file_stem().and_then(|stem| stem.to_str()) {
            aliases.push(normalize_app_key(stem));
        }
        if let Some(program) = app.exec.split_whitespace().next()
            && let Some(program) = std::path::Path::new(program)
                .file_name()
                .and_then(|name| name.to_str())
        {
            aliases.push(normalize_app_key(program));
        }

        for alias in aliases {
            if alias.is_empty() {
                continue;
            }
            icons
                .entry(alias.clone())
                .or_insert_with(|| icon_path.clone());
            if let Some(short) = alias.rsplit('.').next() {
                icons
                    .entry(short.to_string())
                    .or_insert_with(|| icon_path.clone());
            }
        }
    }
    icons
}

fn app_icon(
    icon_path: Option<PathBuf>,
    fallback_label: &str,
    size: gpui::Pixels,
    background: gpui::Hsla,
    foreground: gpui::Hsla,
) -> gpui::AnyElement {
    if let Some(icon_path) = icon_path {
        div()
            .w(size)
            .h(size)
            .flex_none()
            .items_center()
            .justify_center()
            .rounded_xl()
            .overflow_hidden()
            .bg(background)
            .border_1()
            .border_color(foreground.opacity(0.12))
            .shadow_md()
            .child(
                img(icon_path)
                    .w(size)
                    .h(size)
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element()
    } else {
        let initial = fallback_label
            .chars()
            .find(|character| character.is_alphanumeric())
            .map(|character| character.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string());
        div()
            .w(size)
            .h(size)
            .flex_none()
            .items_center()
            .justify_center()
            .rounded_xl()
            .bg(background)
            .text_color(foreground)
            .font_semibold()
            .shadow_md()
            .child(initial)
            .into_any_element()
    }
}

/// Interactive Niri workspace filmstrip overview surface.
pub struct WorkspaceOverview {
    workspaces: Vec<WorkspaceInfo>,
    windows: Vec<WindowInfo>,
    active_workspace_id: Option<u64>,
    selected_window_id: Option<u64>,
    phase: OverviewPhase,
    generation: u64,
    close_reason: Option<OverviewCloseReason>,
    focus_handle: Option<FocusHandle>,
    instance_id: u64,
    reduced_motion: bool,
    wallpaper_path: Option<PathBuf>,
    wallpaper_preview: Option<Arc<Image>>,
    app_icons: HashMap<String, PathBuf>,
    drag_target_workspace_id: Option<u64>,
    workspace_view_start: usize,
}

impl WorkspaceOverview {
    pub fn new(
        workspaces: Vec<WorkspaceInfo>,
        windows: Vec<WindowInfo>,
        active_workspace_id: Option<u64>,
    ) -> Self {
        let selected_window_id = windows
            .iter()
            .find(|w| w.workspace_id == active_workspace_id && w.is_focused)
            .map(|w| w.id);
        let active_workspace_index = active_workspace_id
            .and_then(|active_id| {
                workspaces
                    .iter()
                    .position(|workspace| workspace.id == active_id)
            })
            .unwrap_or(0);
        let workspace_view_start =
            workspace_view_start(0, active_workspace_index, workspaces.len());

        Self {
            workspaces,
            windows,
            active_workspace_id,
            selected_window_id,
            phase: OverviewPhase::Entering,
            generation: 0,
            close_reason: None,
            focus_handle: None,
            instance_id: 0,
            reduced_motion: false,
            wallpaper_path: None,
            wallpaper_preview: None,
            app_icons: HashMap::new(),
            drag_target_workspace_id: None,
            workspace_view_start,
        }
    }

    pub fn new_from_snapshot(snapshot: Arc<CompositorSnapshot>) -> Self {
        Self::new(
            snapshot.workspaces.clone(),
            snapshot.windows.clone(),
            snapshot.focused_workspace_id,
        )
    }

    /// Creates an offline/empty WorkspaceOverview for testing.
    pub fn new_offline() -> Self {
        Self::new(
            vec![
                WorkspaceInfo {
                    id: 1,
                    name: Some("1".into()),
                    idx: 1,
                    is_active: true,
                    is_focused: true,
                    is_urgent: false,
                    output_name: Some("HDMI-1".into()),
                    active_window_id: Some(101),
                },
                WorkspaceInfo {
                    id: 2,
                    name: Some("2".into()),
                    idx: 2,
                    is_active: false,
                    is_focused: false,
                    is_urgent: false,
                    output_name: Some("HDMI-1".into()),
                    active_window_id: None,
                },
            ],
            vec![WindowInfo {
                id: 101,
                title: Some("Terminal".into()),
                app_id: Some("foot".into()),
                workspace_id: Some(1),
                is_focused: true,
                is_floating: false,
                is_urgent: false,
            }],
            Some(1),
        )
    }

    pub fn update_snapshot(&mut self, snapshot: Arc<CompositorSnapshot>, cx: &mut Context<Self>) {
        self.workspaces = snapshot.workspaces.clone();
        self.windows = snapshot.windows.clone();
        self.active_workspace_id = snapshot.focused_workspace_id;
        let active_workspace_index = self
            .active_workspace_id
            .and_then(|active_id| {
                self.workspaces
                    .iter()
                    .position(|workspace| workspace.id == active_id)
            })
            .unwrap_or(0);
        let next_view_start = workspace_view_start(
            self.workspace_view_start,
            active_workspace_index,
            self.workspaces.len(),
        );
        if next_view_start != self.workspace_view_start {
            self.workspace_view_start = next_view_start;
        }
        if !self
            .windows
            .iter()
            .any(|w| Some(w.id) == self.selected_window_id)
        {
            self.selected_window_id = snapshot.focused_window_id;
        }
        cx.notify();
    }

    fn focus_adjacent_workspace(
        &mut self,
        workspace_ids: &[u64],
        forward: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some((target_id, target_index)) =
            adjacent_workspace(workspace_ids, self.active_workspace_id, forward)
            && ShellRuntime::overview_focus_workspace(cx, target_id).is_ok()
        {
            self.active_workspace_id = Some(target_id);
            let next_view_start =
                workspace_view_start(self.workspace_view_start, target_index, workspace_ids.len());
            self.workspace_view_start = next_view_start;
            cx.notify();
        }
    }

    pub fn update_wallpaper_path(
        &mut self,
        wallpaper_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.wallpaper_path != wallpaper_path {
            self.wallpaper_path = wallpaper_path;
            self.wallpaper_preview = None;
            self.load_wallpaper_preview(cx);
            cx.notify();
        }
    }

    fn load_wallpaper_preview(&mut self, cx: &mut Context<Self>) {
        let Some(wallpaper_path) = self.wallpaper_path.clone() else {
            return;
        };
        let requested_path = wallpaper_path.clone();
        let load_task = cx
            .background_executor()
            .spawn(async move { blurred_wallpaper_preview(&wallpaper_path) });
        cx.spawn(async move |this, cx| {
            let preview = load_task.await;
            cx.update(|cx| {
                let Some(entity) = this.upgrade() else {
                    return;
                };
                entity.update(cx, |view, cx| {
                    if view.wallpaper_path.as_ref() == Some(&requested_path) {
                        view.wallpaper_preview = preview;
                        cx.notify();
                    }
                });
            });
        })
        .detach();
    }

    pub fn selected_window_id(&self) -> Option<u64> {
        self.selected_window_id
    }

    pub fn phase(&self) -> OverviewPhase {
        self.phase
    }

    pub fn close_reason(&self) -> Option<OverviewCloseReason> {
        self.close_reason
    }

    /// Begin the exit animation. When the animation timer fires, the runtime
    /// will remove the window surface.
    pub fn begin_close(&mut self, reason: OverviewCloseReason, cx: &mut Context<Self>) {
        if self.phase == OverviewPhase::Exiting {
            return; // already exiting, ignore duplicate
        }
        self.phase = OverviewPhase::Exiting;
        self.generation += 1;
        let gen_id = self.generation;
        self.close_reason = Some(reason);
        let reduced_motion = self.reduced_motion;
        cx.notify();

        // Schedule teardown after exit animation completes.
        let weak_entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(if reduced_motion {
                    Duration::ZERO
                } else {
                    EXIT_DURATION
                })
                .await;
            cx.update(|cx| {
                if let Some(entity) = weak_entity.upgrade()
                    && entity.read(cx).generation == gen_id
                {
                    let instance_id = entity.read(cx).instance_id;
                    ShellRuntime::finish_overview_close(cx, reason, instance_id);
                }
            });
        })
        .detach();
    }

    /// Immediately mark phase as Visible (called after entry animation settles).
    pub fn mark_visible(&mut self, gen_id: u64) {
        if self.generation == gen_id && self.phase == OverviewPhase::Entering {
            self.phase = OverviewPhase::Visible;
        }
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            cx.stop_propagation();
            self.begin_close(OverviewCloseReason::Cancel, cx);
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<shilpo_ui::Root> {
        let snapshot = ShellRuntime::compositor_snapshot(cx);
        let instance_id = ShellRuntime::begin_overview_instance(cx);
        let reduced_motion = ShellRuntime::overview_reduced_motion(cx);
        let wallpaper_path = ShellRuntime::overview_wallpaper_path(cx);
        let app_icons = build_app_icon_index(ShellRuntime::overview_applications(cx));

        window.on_window_should_close(cx, move |_, cx| {
            ShellRuntime::forget_overview(cx, instance_id);
            true
        });

        let overview = cx.new(|cx| {
            let mut ov = Self::new_from_snapshot(snapshot);
            ov.instance_id = instance_id;
            ov.reduced_motion = reduced_motion;
            ov.wallpaper_path = wallpaper_path;
            ov.load_wallpaper_preview(cx);
            ov.app_icons = app_icons;
            let focus_handle = cx.focus_handle();
            focus_handle.focus(window, cx);
            ov.focus_handle = Some(focus_handle);
            // Schedule entry → visible transition.
            let gen_id = ov.generation;
            if reduced_motion {
                ov.phase = OverviewPhase::Visible;
            } else {
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(ENTER_DURATION).await;
                    cx.update(|cx| {
                        if let Some(entity) = this.upgrade() {
                            entity.update(cx, |view: &mut Self, cx| {
                                view.mark_visible(gen_id);
                                cx.notify();
                            });
                        }
                    });
                })
                .detach();
            }
            ov
        });
        ShellRuntime::register_overview_entity(cx, overview.clone());
        cx.new(|cx| {
            shilpo_ui::Root::new(overview, window, cx)
                .bordered(false)
                .bg(gpui::transparent_black())
        })
    }

    pub fn select_next_window(&mut self) {
        if self.windows.is_empty() {
            return;
        }
        let current_index = self
            .selected_window_id
            .and_then(|id| self.windows.iter().position(|w| w.id == id))
            .unwrap_or(0);
        let next_index = (current_index + 1) % self.windows.len();
        self.selected_window_id = Some(self.windows[next_index].id);
    }

    fn is_interactive(&self) -> bool {
        self.phase != OverviewPhase::Exiting
    }

    fn icon_path_for_app(&self, app_id: Option<&str>) -> Option<PathBuf> {
        let app_id = normalize_app_key(app_id?);
        self.app_icons
            .get(&app_id)
            .or_else(|| {
                app_id
                    .rsplit('.')
                    .next()
                    .and_then(|short| self.app_icons.get(short))
            })
            .or_else(|| {
                self.app_icons.iter().find_map(|(key, path)| {
                    (app_id.ends_with(key)
                        || key.ends_with(&app_id)
                        || key.starts_with(&format!("{app_id}-")))
                    .then_some(path)
                })
            })
            .cloned()
    }
}

impl Render for WorkspaceOverview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_interactive = self.is_interactive();
        let generation = self.generation;
        let phase = self.phase;

        // M3 scrim at ~48% opacity
        let scrim_color = theme.scrim.opacity(0.48);
        let card_bg = theme.surface_container_high;
        let border_color = theme.outline_variant;
        let viewport = window.viewport_size();
        let has_active_drag = cx.has_active_drag();

        // ── Scrim backdrop ──────────────────────────────────────────────
        let scrim = div()
            .id("overview_scrim")
            .absolute()
            .top_0()
            .left_0()
            .w(viewport.width)
            .h(viewport.height)
            .bg(scrim_color)
            .when(is_interactive, |s| {
                s.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, _, cx| {
                        cx.stop_propagation();
                        view.begin_close(OverviewCloseReason::Cancel, cx);
                    }),
                )
            });

        // ── Wallpaper-backed workspace previews ─────────────────────────
        let all_workspaces: Vec<_> = self.workspaces.iter().collect();
        let visible_workspace_count = all_workspaces.len();
        let max_view_start = visible_workspace_count.saturating_sub(MAX_VISIBLE_WORKSPACES);
        self.workspace_view_start = self.workspace_view_start.min(max_view_start);
        let render_range =
            workspace_render_range(self.workspace_view_start, visible_workspace_count);
        let rendered_workspace_count = render_range.len();
        let visible_workspace_ids: Vec<_> = all_workspaces
            .iter()
            .map(|workspace| workspace.id)
            .collect();
        let card_elements: Vec<_> = all_workspaces[render_range]
            .iter()
            .copied()
            .enumerate()
            .map(|(workspace_index, ws)| {
                let ws_id = ws.id;
                let is_first = workspace_index == 0;
                let is_last = workspace_index + 1 == rendered_workspace_count;
                let is_active = self.active_workspace_id == Some(ws_id);
                let is_drag_target =
                    has_active_drag && self.drag_target_workspace_id == Some(ws_id);
                let ws_windows: Vec<_> = self
                    .windows
                    .iter()
                    .filter(|w| w.workspace_id == Some(ws_id))
                    .collect();
                let ws_title = ws.name.clone().unwrap_or_else(|| ws.idx.to_string());
                let window_count = ws_windows.len();
                let wallpaper_source: Option<ImageSource> = self
                    .wallpaper_preview
                    .clone()
                    .map(ImageSource::from)
                    .or_else(|| self.wallpaper_path.clone().map(ImageSource::from));
                let inner_radius = px(4.);
                let top_radius = px(if is_first {
                    PREVIEW_RADIUS
                } else {
                    INTER_WORKSPACE_RADIUS
                });
                let bottom_radius = px(if is_last {
                    PREVIEW_RADIUS
                } else {
                    INTER_WORKSPACE_RADIUS
                });

                // Application icons are window proxies: click focuses, drag moves.
                let window_icons: Vec<_> = ws_windows
                    .into_iter()
                    .enumerate()
                    .map(|(window_index, win)| {
                        let win_id = win.id;
                        let source_workspace_id = win.workspace_id;
                        let win_title: SharedString =
                            win.title.as_deref().unwrap_or("Window").to_string().into();
                        let fallback_label = win.app_id.as_deref().unwrap_or(win_title.as_ref());
                        let icon_path = self.icon_path_for_app(win.app_id.as_deref());
                        let drag = DraggedOverviewWindow {
                            window_id: win_id,
                            source_workspace_id,
                            title: win_title.clone(),
                            icon_path: icon_path.clone(),
                            region_index: window_index,
                            region_count: window_count,
                            top_radius: top_radius.into(),
                            bottom_radius: bottom_radius.into(),
                        };
                        let icon_size = ((PREVIEW_WIDTH / window_count.max(1) as f32) - 24.)
                            .clamp(30., APP_ICON_SIZE);
                        let icon = app_icon(
                            icon_path,
                            fallback_label,
                            px(icon_size),
                            theme.surface_container_highest,
                            theme.on_surface,
                        );
                        div()
                            .id(ElementId::Name(SharedString::from(format!(
                                "overview-window-{}",
                                win.id
                            ))))
                            .h_full()
                            .min_w(px(0.))
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_tl(if window_index == 0 {
                                top_radius
                            } else {
                                inner_radius
                            })
                            .rounded_bl(if window_index == 0 {
                                bottom_radius
                            } else {
                                inner_radius
                            })
                            .rounded_tr(if window_index + 1 == window_count {
                                top_radius
                            } else {
                                inner_radius
                            })
                            .rounded_br(if window_index + 1 == window_count {
                                bottom_radius
                            } else {
                                inner_radius
                            })
                            .when(has_active_drag, |item| item.cursor_grabbing())
                            .when(!has_active_drag, |item| item.cursor_grab())
                            .hover(|item| item.bg(theme.surface_container_high.opacity(0.12)))
                            .active(|item| item.bg(theme.surface_container_high.opacity(0.18)))
                            .role(Role::Button)
                            .aria_label(format!("Focus or drag {}", win_title))
                            .child(icon)
                            .when(is_interactive, |s| {
                                s.on_click(cx.listener(move |view, _, _, cx| {
                                    cx.stop_propagation();
                                    if ShellRuntime::overview_focus_window(cx, win_id).is_ok() {
                                        view.begin_close(OverviewCloseReason::Selection, cx);
                                    }
                                }))
                                .on_drag(drag, move |drag, _, _, cx| cx.new(|_| drag.clone()))
                            })
                            .into_any_element()
                    })
                    .collect();

                let preview = div()
                    .w(px(PREVIEW_WIDTH))
                    .h(px(PREVIEW_HEIGHT))
                    .relative()
                    .rounded_tl(top_radius)
                    .rounded_tr(top_radius)
                    .rounded_bl(bottom_radius)
                    .rounded_br(bottom_radius)
                    .overflow_hidden()
                    .flex_none()
                    .bg(card_bg)
                    .when_some(wallpaper_source, |preview, wallpaper_source| {
                        preview.child(
                            img(wallpaper_source)
                                .absolute()
                                .inset_0()
                                .size_full()
                                .rounded_tl(top_radius)
                                .rounded_tr(top_radius)
                                .rounded_bl(bottom_radius)
                                .rounded_br(bottom_radius)
                                .object_fit(ObjectFit::Cover),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_tl(top_radius)
                            .rounded_tr(top_radius)
                            .rounded_bl(bottom_radius)
                            .rounded_br(bottom_radius)
                            .bg(theme.scrim.opacity(if is_active { 0.24 } else { 0.26 })),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_tl(top_radius)
                            .rounded_tr(top_radius)
                            .rounded_bl(bottom_radius)
                            .rounded_br(bottom_radius)
                            .bg(theme.surface.opacity(0.08)),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_center()
                            .children(window_icons),
                    )
                    .when(is_drag_target, |preview| {
                        preview.child(
                            div()
                                .absolute()
                                .inset_0()
                                .rounded_tl(top_radius)
                                .rounded_tr(top_radius)
                                .rounded_bl(bottom_radius)
                                .rounded_br(bottom_radius)
                                .bg(theme.primary.opacity(0.16)),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_tl(top_radius)
                            .rounded_tr(top_radius)
                            .rounded_bl(bottom_radius)
                            .rounded_br(bottom_radius)
                            .border_1()
                            .when(is_active || is_drag_target, |outline| outline.border_2())
                            .border_color(if is_active || is_drag_target {
                                theme.primary
                            } else {
                                border_color.opacity(0.18)
                            }),
                    );

                preview
                    .id(ElementId::Name(SharedString::from(format!(
                        "workspace-preview-{}",
                        ws_id
                    ))))
                    .shadow_sm()
                    .when(is_drag_target, |preview| preview.shadow_md())
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_label(format!("Focus workspace {}", ws_title))
                    .when(is_interactive, |preview| {
                        preview
                            .on_click(cx.listener(move |view, _, _, cx| {
                                cx.stop_propagation();
                                if ShellRuntime::overview_focus_workspace(cx, ws_id).is_ok() {
                                    view.begin_close(OverviewCloseReason::Selection, cx);
                                }
                            }))
                            .can_drop(|value, _, _| value.is::<DraggedOverviewWindow>())
                            .on_drag_move(cx.listener(
                                move |view, event: &DragMoveEvent<DraggedOverviewWindow>, _, cx| {
                                    if event.bounds.contains(&event.event.position)
                                        && view.drag_target_workspace_id != Some(ws_id)
                                    {
                                        view.drag_target_workspace_id = Some(ws_id);
                                        cx.notify();
                                    }
                                },
                            ))
                            .on_hover(cx.listener(move |view, hovering: &bool, _, cx| {
                                if !*hovering && view.drag_target_workspace_id == Some(ws_id) {
                                    view.drag_target_workspace_id = None;
                                    cx.notify();
                                }
                            }))
                            .on_drop(cx.listener(
                                move |view, drag: &DraggedOverviewWindow, _, cx| {
                                    cx.stop_propagation();
                                    view.drag_target_workspace_id = None;
                                    if drag.source_workspace_id != Some(ws_id) {
                                        let _ = ShellRuntime::overview_move_window(
                                            cx,
                                            drag.window_id,
                                            ws_id,
                                        );
                                    }
                                    cx.notify();
                                },
                            ))
                    })
                    .into_any_element()
            })
            .collect();

        // ── Expressive vertical filmstrip stage ─────────────────────────
        let focus_handle = self
            .focus_handle
            .as_ref()
            .expect("workspace overview must be created through WorkspaceOverview::view")
            .clone();
        let stage_max_height = (viewport.height.as_f32() - 24.0).max(240.0);
        let available_scroll_height =
            (stage_max_height - (STAGE_VERTICAL_PADDING * 2.0) - 2.0).max(220.0);
        let scroll_height = available_scroll_height.min(filmstrip_height(visible_workspace_count));
        let stage = div()
            .id("overview_stage")
            .role(Role::Dialog)
            .aria_label("Workspace Overview")
            .track_focus(&focus_handle)
            .focus_trap("workspace-overview-focus-trap", &focus_handle)
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .w(px(PREVIEW_WIDTH
                + STAGE_HORIZONTAL_PADDING * 2.0
                + STAGE_BORDER_WIDTH * 2.0))
            .max_h(px(stage_max_height))
            .px(px(STAGE_HORIZONTAL_PADDING))
            .py(px(STAGE_VERTICAL_PADDING))
            .flex()
            .flex_col()
            .items_center()
            .bg(theme.surface_container.opacity(0.98))
            .border_1()
            .border_color(theme.outline_variant.opacity(0.62))
            .rounded(px(32.))
            .shadow_lg()
            .child(
                div()
                    .id("overview-workspace-filmstrip")
                    .w(px(PREVIEW_WIDTH))
                    .max_h(px(scroll_height))
                    .gap(px(PREVIEW_GAP))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_scroll_wheel(cx.listener(
                        move |view, event: &ScrollWheelEvent, window, cx| {
                            let delta_y = event.delta.pixel_delta(window.line_height()).y;
                            if delta_y == px(0.) {
                                return;
                            }
                            cx.stop_propagation();
                            if !has_active_drag {
                                view.focus_adjacent_workspace(
                                    &visible_workspace_ids,
                                    delta_y < px(0.),
                                    cx,
                                );
                            }
                        },
                    ))
                    .children(card_elements),
            )
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
                view.handle_key_down(event, cx);
            }));

        let stage = if !self.reduced_motion && phase != OverviewPhase::Visible {
            let (duration, from, to) = match phase {
                OverviewPhase::Entering => (ENTER_DURATION, 0.94_f32, 1.0_f32),
                OverviewPhase::Exiting => (EXIT_DURATION, 1.0_f32, 0.94_f32),
                OverviewPhase::Visible => unreachable!(),
            };
            let easing = match phase {
                OverviewPhase::Entering => cubic_bezier(0.05, 0.7, 0.1, 1.0),
                OverviewPhase::Exiting => cubic_bezier(0.3, 0.0, 0.8, 0.15),
                OverviewPhase::Visible => unreachable!(),
            };
            stage
                .with_animation(
                    ElementId::NamedInteger("overview-stage-motion".into(), generation),
                    Animation::new(duration).with_easing(easing),
                    move |stage, delta| {
                        let scale = from + (to - from) * delta;
                        stage
                            .px(px(STAGE_HORIZONTAL_PADDING * scale))
                            .py(px(STAGE_VERTICAL_PADDING * scale))
                    },
                )
                .into_any_element()
        } else {
            stage.into_any_element()
        };

        // ── Root container ──────────────────────────────────────────────
        let root = div()
            .id("workspace_overview_root")
            .size_full()
            .relative()
            .child(scrim)
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(stage),
            );

        // ── Animate entry / exit ────────────────────────────────────────
        let (anim_duration, fade_from, fade_to) = match phase {
            OverviewPhase::Entering => (ENTER_DURATION, 0.0_f32, 1.0_f32),
            OverviewPhase::Exiting => (EXIT_DURATION, 1.0_f32, 0.0_f32),
            OverviewPhase::Visible => {
                return root.into_any_element();
            }
        };

        // M3 emphasized-decelerate for enter, emphasized-accelerate for exit
        let easing = match phase {
            OverviewPhase::Entering => cubic_bezier(0.05, 0.7, 0.1, 1.0),
            OverviewPhase::Exiting => cubic_bezier(0.3, 0.0, 0.8, 0.15),
            OverviewPhase::Visible => unreachable!(),
        };

        if self.reduced_motion {
            return root
                .opacity(if phase == OverviewPhase::Exiting {
                    0.0
                } else {
                    1.0
                })
                .into_any_element();
        }

        let animation = Animation::new(anim_duration).with_easing(easing);

        root.with_animation(
            ElementId::NamedInteger("overview-transition".into(), generation),
            animation,
            move |el, delta| {
                let opacity = fade_from + (fade_to - fade_from) * delta;
                el.opacity(opacity)
            },
        )
        .into_any_element()
    }
}

impl Focusable for WorkspaceOverview {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle
            .clone()
            .expect("workspace overview must own a focus handle")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_overview_navigation() {
        let mut overview = WorkspaceOverview::new_offline();
        assert_eq!(overview.selected_window_id(), Some(101));
        overview.select_next_window();
        assert_eq!(overview.selected_window_id(), Some(101));
    }

    #[test]
    fn test_overview_phase_lifecycle() {
        let overview = WorkspaceOverview::new_offline();
        assert_eq!(overview.phase(), OverviewPhase::Entering);
        assert!(overview.is_interactive());
    }

    #[test]
    fn test_overview_generation_increments_on_close() {
        let mut overview = WorkspaceOverview::new_offline();
        assert_eq!(overview.generation, 0);
        overview.phase = OverviewPhase::Exiting;
        overview.generation += 1;
        assert_eq!(overview.generation, 1);
        assert!(!overview.is_interactive());
    }

    #[test]
    fn filmstrip_height_is_capped_at_three_workspaces() {
        assert_eq!(filmstrip_height(0), 0.0);
        assert_eq!(filmstrip_height(1), PREVIEW_HEIGHT);
        assert_eq!(
            filmstrip_height(3),
            PREVIEW_HEIGHT * 3.0 + PREVIEW_GAP * 2.0
        );
        assert_eq!(filmstrip_height(4), filmstrip_height(3));
    }

    #[test]
    fn adjacent_workspace_moves_by_one_and_stops_at_each_end() {
        let workspace_ids = [10, 20, 30, 40];

        assert_eq!(
            adjacent_workspace(&workspace_ids, Some(10), true),
            Some((20, 1))
        );
        assert_eq!(
            adjacent_workspace(&workspace_ids, Some(30), true),
            Some((40, 3))
        );
        assert_eq!(adjacent_workspace(&workspace_ids, Some(40), true), None);
        assert_eq!(
            adjacent_workspace(&workspace_ids, Some(30), false),
            Some((20, 1))
        );
        assert_eq!(adjacent_workspace(&workspace_ids, Some(10), false), None);
    }

    #[test]
    fn workspace_view_tracks_target_without_exceeding_three_cards() {
        assert_eq!(workspace_view_start(0, 0, 5), 0);
        assert_eq!(workspace_view_start(0, 2, 5), 0);
        assert_eq!(workspace_view_start(0, 3, 5), 1);
        assert_eq!(workspace_view_start(1, 4, 5), 2);
        assert_eq!(workspace_view_start(2, 1, 5), 1);
        assert_eq!(workspace_view_start(2, 0, 2), 0);
    }

    #[test]
    fn workspace_render_range_contains_exactly_three_whole_cards() {
        assert_eq!(workspace_render_range(0, 5), 0..3);
        assert_eq!(workspace_render_range(1, 5), 1..4);
        assert_eq!(workspace_render_range(2, 5), 2..5);
        assert_eq!(workspace_render_range(0, 2), 0..2);
    }

    #[test]
    fn app_icon_index_matches_desktop_and_short_app_ids() {
        let icon_path = PathBuf::from("/tmp/org.example.Terminal.svg");
        let index = build_app_icon_index(vec![Application {
            name: "Example Terminal".into(),
            exec: "/usr/bin/example-terminal --new-window".into(),
            icon: Some("org.example.Terminal".into()),
            icon_path: Some(icon_path.clone()),
            description: None,
            categories: Vec::new(),
            desktop_file: PathBuf::from("/usr/share/applications/org.example.Terminal.desktop"),
            working_dir: None,
            terminal: false,
            try_exec: None,
        }]);

        assert_eq!(index.get("org.example.terminal"), Some(&icon_path));
        assert_eq!(index.get("terminal"), Some(&icon_path));
        assert_eq!(index.get("example-terminal"), Some(&icon_path));

        let mut overview = WorkspaceOverview::new_offline();
        overview
            .app_icons
            .insert("jetbrains-rustrover-install-id".into(), icon_path.clone());
        assert_eq!(
            overview.icon_path_for_app(Some("jetbrains-rustrover")),
            Some(icon_path)
        );
    }
}
