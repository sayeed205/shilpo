use std::{cell::Cell, rc::Rc, time::Duration};

use crate::actions::ActionInvocation;
use crate::runtime::ShellRuntime;
use gpui::{
    Animation, AnimationExt as _, App, ElementId, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, RenderOnce, Role, StatefulInteractiveElement, StyleRefinement, Styled,
    Window, div, px,
};
use shilpo_services::{CompositorConnection, WorkspaceInfo};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex, tooltip::Tooltip};

fn workspace_actions_enabled(connection: &CompositorConnection) -> bool {
    matches!(connection, CompositorConnection::Ready)
}

fn workspace_status_label(connection: &CompositorConnection) -> Option<&'static str> {
    match connection {
        CompositorConnection::Connecting => Some("Connecting..."),
        CompositorConnection::Reconnecting { .. } => Some("Reconnecting"),
        CompositorConnection::Stopped => Some("Compositor Unavailable"),
        CompositorConnection::Ready => None,
    }
}

const WORKSPACE_SLOT_SIZE: f32 = 26.;
const WORKSPACE_ACTIVE_MARGIN: f32 = 2.;
const WORKSPACE_INDICATOR_SIZE: f32 = WORKSPACE_SLOT_SIZE - (WORKSPACE_ACTIVE_MARGIN * 2.);
const WORKSPACE_DOT_SIZE: f32 = WORKSPACE_SLOT_SIZE * 0.18;
const WORKSPACE_MOTION_DURATION: Duration = Duration::from_millis(300);
const WORKSPACE_LEADING_EDGE_DURATION: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq)]
struct WorkspaceIndicatorGeometry {
    x: Pixels,
    width: Pixels,
}

fn indicator_target(index: usize) -> WorkspaceIndicatorGeometry {
    WorkspaceIndicatorGeometry {
        x: px(index as f32 * WORKSPACE_SLOT_SIZE + WORKSPACE_ACTIVE_MARGIN),
        width: px(WORKSPACE_INDICATOR_SIZE),
    }
}

fn out_sine(progress: f32) -> f32 {
    (progress.clamp(0., 1.) * std::f32::consts::FRAC_PI_2).sin()
}

fn lerp_pixels(from: Pixels, target: Pixels, progress: f32) -> Pixels {
    let from: f32 = from.into();
    let target: f32 = target.into();
    px(from + (target - from) * progress)
}

fn calculate_stretching_geometry(
    from: WorkspaceIndicatorGeometry,
    target: WorkspaceIndicatorGeometry,
    delta: f32,
) -> WorkspaceIndicatorGeometry {
    if delta >= 1. {
        return target;
    }

    // Inir's AnimatedTabIndexPair moves the leading edge in 100 ms and the
    // following edge in 300 ms, both with OutSine easing. Keeping the edges
    // independent creates the elastic capsule without moving foreground dots.
    let elapsed = WORKSPACE_MOTION_DURATION.as_secs_f32() * delta.clamp(0., 1.);
    let fast = out_sine(elapsed / WORKSPACE_LEADING_EDGE_DURATION.as_secs_f32());
    let slow = out_sine(elapsed / WORKSPACE_MOTION_DURATION.as_secs_f32());

    let from_left = from.x;
    let from_right = from.x + from.width;
    let target_left = target.x;
    let target_right = target.x + target.width;
    let moving_right = target.x >= from.x;

    let (left, right) = if moving_right {
        (
            lerp_pixels(from_left, target_left, slow),
            lerp_pixels(from_right, target_right, fast),
        )
    } else {
        (
            lerp_pixels(from_left, target_left, fast),
            lerp_pixels(from_right, target_right, slow),
        )
    };

    WorkspaceIndicatorGeometry {
        x: left,
        width: (right - left).max(px(WORKSPACE_INDICATOR_SIZE)),
    }
}

#[derive(Clone)]
struct WorkspaceMotionState {
    target_index: usize,
    from: WorkspaceIndicatorGeometry,
    target: WorkspaceIndicatorGeometry,
    current: Rc<Cell<WorkspaceIndicatorGeometry>>,
    active_generation: Rc<Cell<u64>>,
    generation: u64,
    duration: Duration,
    active: bool,
}

impl WorkspaceMotionState {
    fn new(target_index: usize, target: WorkspaceIndicatorGeometry) -> Self {
        Self {
            target_index,
            from: target,
            target,
            current: Rc::new(Cell::new(target)),
            active_generation: Rc::new(Cell::new(0)),
            generation: 0,
            duration: WORKSPACE_MOTION_DURATION,
            active: false,
        }
    }

    fn retarget(&mut self, target_index: usize, target: WorkspaceIndicatorGeometry) -> u64 {
        if self.target_index == target_index && self.target == target {
            return self.generation;
        }

        self.generation = self.generation.wrapping_add(1);
        self.from = self.current.get();
        self.target_index = target_index;
        self.target = target;
        self.duration = WORKSPACE_MOTION_DURATION;
        self.active = true;
        self.active_generation.set(self.generation);
        self.generation
    }
}

/// Workspaces widget for status bar consuming compositor snapshots.
#[derive(IntoElement)]
pub struct WorkspacesWidget {
    id: ElementId,
    workspaces: Vec<WorkspaceInfo>,
    connection: CompositorConnection,
    style: StyleRefinement,
}

impl WorkspacesWidget {
    pub fn new(
        id: impl Into<ElementId>,
        workspaces: Vec<WorkspaceInfo>,
        connection: CompositorConnection,
    ) -> Self {
        Self {
            id: id.into(),
            workspaces,
            connection,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for WorkspacesWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn render_workspace_dot(ws: &WorkspaceInfo, is_ready: bool, cx: &mut App) -> gpui::AnyElement {
    let is_active = ws.is_active || ws.is_focused;
    let is_occupied = ws.active_window_id.is_some();
    let label = ws.name.clone().unwrap_or_else(|| ws.idx.to_string());
    let ws_id = ws.id;

    let dot_color = if is_active {
        cx.theme().on_primary
    } else if is_occupied {
        cx.theme().on_secondary_container
    } else {
        cx.theme().on_surface_variant.opacity(0.4)
    };

    let base_pill = div()
        .id(("ws", ws.id))
        .role(Role::Button)
        .aria_label(format!("Workspace {}", label))
        .w(px(WORKSPACE_SLOT_SIZE))
        .h(px(WORKSPACE_SLOT_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(WORKSPACE_DOT_SIZE))
                .h(px(WORKSPACE_DOT_SIZE))
                .rounded_full()
                .bg(dot_color),
        );

    if is_ready {
        base_pill
            .cursor_pointer()
            .hover(|s| if is_active { s } else { s.opacity(0.8) })
            .on_click(move |_, _, cx| {
                let _ = ShellRuntime::dispatch_action(cx, ActionInvocation::FocusWorkspace(ws_id));
            })
            .on_mouse_down(MouseButton::Right, move |_, _, cx| {
                cx.stop_propagation();
                ShellRuntime::open_or_focus_overview(cx);
            })
            .into_any_element()
    } else {
        base_pill.into_any_element()
    }
}

impl RenderOnce for WorkspacesWidget {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_ready = workspace_actions_enabled(&self.connection);
        let is_connecting = matches!(self.connection, CompositorConnection::Connecting);
        let is_stopped = workspace_status_label(&self.connection) == Some("Compositor Unavailable");

        let mut items: Vec<gpui::AnyElement> = Vec::new();
        let mut occupied_backgrounds: Vec<gpui::AnyElement> = Vec::new();
        let mut active_workspace_index: Option<usize> = None;

        if is_connecting {
            items.push(
                div()
                    .id("ws_connecting")
                    .role(Role::Status)
                    .aria_label("Compositor connecting")
                    .h(px(24.))
                    .px_2_5()
                    .rounded_full()
                    .bg(cx.theme().surface_container_highest.opacity(0.5))
                    .text_color(cx.theme().on_surface_variant.opacity(0.6))
                    .text_xs()
                    .font_bold()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Connecting...")
                    .into_any_element(),
            );
        } else if is_stopped {
            let last_err = match &self.connection {
                CompositorConnection::Stopped => "Compositor is unavailable".to_string(),
                _ => String::new(),
            };
            items.push(
                div()
                    .id("ws_stopped")
                    .role(Role::Status)
                    .aria_label("Compositor unavailable")
                    .tooltip(move |window, cx| Tooltip::new(last_err.clone()).build(window, cx))
                    .h(px(24.))
                    .px_2_5()
                    .rounded_full()
                    .bg(cx.theme().error_container)
                    .text_color(cx.theme().on_error_container)
                    .text_xs()
                    .font_bold()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Compositor Unavailable")
                    .into_any_element(),
            );
        } else if is_ready && self.workspaces.is_empty() {
            items.push(
                div()
                    .id("ws_empty")
                    .role(Role::Status)
                    .aria_label("No workspaces available")
                    .h(px(24.))
                    .min_w(px(26.))
                    .px_2()
                    .rounded_full()
                    .bg(cx.theme().surface_container_highest.opacity(0.5))
                    .text_color(cx.theme().on_surface_variant.opacity(0.5))
                    .text_xs()
                    .font_bold()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("1")
                    .into_any_element(),
            );
        } else if !self.workspaces.is_empty() {
            active_workspace_index = self
                .workspaces
                .iter()
                .position(|ws| ws.is_active || ws.is_focused);

            let occupied: Vec<bool> = self
                .workspaces
                .iter()
                .map(|ws| ws.active_window_id.is_some())
                .collect();

            for (index, is_occupied) in occupied.iter().copied().enumerate() {
                if !is_occupied {
                    continue;
                }

                let joined_left = index > 0 && occupied[index - 1];
                let joined_right = occupied.get(index + 1).copied().unwrap_or(false);
                let left_radius = if joined_left {
                    px(0.)
                } else {
                    px(WORKSPACE_SLOT_SIZE / 2.)
                };
                let right_radius = if joined_right {
                    px(0.)
                } else {
                    px(WORKSPACE_SLOT_SIZE / 2.)
                };

                occupied_backgrounds.push(
                    div()
                        .absolute()
                        .top(px(0.))
                        .left(px(index as f32 * WORKSPACE_SLOT_SIZE))
                        .w(px(WORKSPACE_SLOT_SIZE))
                        .h(px(WORKSPACE_SLOT_SIZE))
                        .rounded_tl(left_radius)
                        .rounded_bl(left_radius)
                        .rounded_tr(right_radius)
                        .rounded_br(right_radius)
                        .bg(cx.theme().secondary_container.opacity(0.6))
                        .into_any_element(),
                );
            }

            items.extend(
                self.workspaces
                    .iter()
                    .map(|ws| render_workspace_dot(ws, is_ready, cx)),
            );
        }

        let active_indicator_element = if let Some(active_idx) = active_workspace_index {
            let target_geom = indicator_target(active_idx);
            let motion_key = format!("workspace-motion:{}", self.id);
            let animation_name = format!("workspace-indicator-motion:{}", self.id);

            let motion = window.use_keyed_state(motion_key, cx, |_, _| {
                WorkspaceMotionState::new(active_idx, target_geom)
            });

            let snapshot = motion.read(cx).clone();

            if snapshot.target_index != active_idx || snapshot.target != target_geom {
                let generation =
                    motion.update(cx, |state, _| state.retarget(active_idx, target_geom));
                let duration = motion.read(cx).duration;
                let motion = motion.clone();
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(duration).await;
                    motion.update(cx, |state, cx| {
                        if state.generation == generation {
                            state.active = false;
                            state.current.set(target_geom);
                            cx.notify();
                        }
                    });
                })
                .detach();
            }

            let state = motion.read(cx);
            let from_geometry = state.from;
            let current_geometry = state.current.clone();
            let active_generation = state.active_generation.clone();
            let generation = state.generation;
            let duration = state.duration;
            let active = state.active;

            let pill = div()
                .absolute()
                .top(px(WORKSPACE_ACTIVE_MARGIN))
                .left(target_geom.x)
                .w(target_geom.width)
                .h(px(WORKSPACE_INDICATOR_SIZE))
                .rounded_full()
                .bg(cx.theme().primary)
                .flex();

            if active {
                pill.with_animation(
                    ElementId::NamedInteger(animation_name.into(), generation),
                    Animation::new(duration),
                    move |pill, delta| {
                        let geometry =
                            calculate_stretching_geometry(from_geometry, target_geom, delta);
                        if active_generation.get() == generation {
                            current_geometry.set(geometry);
                        }
                        pill.left(geometry.x).w(geometry.width)
                    },
                )
                .into_any_element()
            } else {
                pill.into_any_element()
            }
        } else {
            div().into_any_element()
        };

        if let CompositorConnection::Reconnecting {
            attempt,
            ref last_error,
        } = self.connection
        {
            let error_msg = last_error
                .clone()
                .unwrap_or_else(|| "Attempting to reconnect to compositor...".into());
            items.push(
                div()
                    .id("ws_reconnect_indicator")
                    .role(Role::Status)
                    .aria_label(format!("Compositor reconnecting attempt {}", attempt))
                    .tooltip(move |window, cx| Tooltip::new(error_msg.clone()).build(window, cx))
                    .flex()
                    .ml_1()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .h(px(24.))
                    .rounded_full()
                    .bg(cx.theme().tertiary_container)
                    .text_color(cx.theme().on_tertiary_container)
                    .text_xs()
                    .child(Icon::new(IconName::Info).size(px(12.)))
                    .child(format!("Reconnecting ({})", attempt))
                    .into_any_element(),
            );
        }

        div()
            .id(self.id)
            .relative()
            .flex()
            .items_center()
            .children(occupied_backgrounds)
            .child(active_indicator_element)
            .child(h_flex().items_center().gap(px(0.)).children(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_f32(value: Pixels) -> f32 {
        value.into()
    }

    #[test]
    fn workspace_geometry_matches_inir_slot_grid() {
        let first = indicator_target(0);
        let third = indicator_target(2);

        assert_eq!(as_f32(first.x), 2.);
        assert_eq!(as_f32(first.width), 22.);
        assert_eq!(as_f32(third.x), 54.);
    }

    #[test]
    fn workspace_indicator_stretches_then_settles_on_target() {
        let from = indicator_target(0);
        let target = indicator_target(2);
        let halfway = calculate_stretching_geometry(from, target, 0.5);
        let settled = calculate_stretching_geometry(from, target, 1.);

        assert!(as_f32(halfway.width) > WORKSPACE_INDICATOR_SIZE);
        assert!(as_f32(halfway.x) > as_f32(from.x));
        assert_eq!(settled, target);
    }

    #[test]
    fn workspace_actions_only_enable_when_ready() {
        assert!(workspace_actions_enabled(&CompositorConnection::Ready));
        assert!(!workspace_actions_enabled(
            &CompositorConnection::Connecting
        ));
        assert!(!workspace_actions_enabled(
            &CompositorConnection::Reconnecting {
                attempt: 1,
                last_error: None,
            }
        ));
        assert!(!workspace_actions_enabled(&CompositorConnection::Stopped));
    }

    #[test]
    fn workspace_status_labels_cover_connection_states() {
        assert_eq!(
            workspace_status_label(&CompositorConnection::Connecting),
            Some("Connecting...")
        );
        assert_eq!(
            workspace_status_label(&CompositorConnection::Reconnecting {
                attempt: 2,
                last_error: Some("socket closed".into()),
            }),
            Some("Reconnecting")
        );
        assert_eq!(
            workspace_status_label(&CompositorConnection::Stopped),
            Some("Compositor Unavailable")
        );
        assert_eq!(workspace_status_label(&CompositorConnection::Ready), None);
    }
}
