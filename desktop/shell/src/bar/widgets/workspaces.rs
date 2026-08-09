use crate::actions::ActionInvocation;
use crate::bar::widgets::pill_strip::PillOrientation;
use crate::runtime::ShellRuntime;
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, Role,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, px,
};
use shilpo_services::{CompositorConnection, WorkspaceInfo};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex, tooltip::Tooltip, v_flex};

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
const WORKSPACE_DOT_SIZE: f32 = WORKSPACE_SLOT_SIZE * 0.18;
const WORKSPACE_OVERVIEW_SPLIT_GAP: f32 = 2.;

#[derive(Clone, Copy, Debug, PartialEq)]
struct OccupiedBackgroundGeometry {
    position: f32,
    size: f32,
    start_radius: f32,
    end_radius: f32,
}

fn occupied_background_geometry(
    index: usize,
    occupied: &[bool],
    overview_open: bool,
    active_workspace_index: Option<usize>,
) -> OccupiedBackgroundGeometry {
    let occupied_left = index > 0 && occupied[index - 1];
    let occupied_right = occupied.get(index + 1).copied().unwrap_or(false);
    let is_active = active_workspace_index == Some(index);
    let split_left = overview_open
        && occupied_left
        && (is_active || active_workspace_index == index.checked_sub(1));
    let split_right =
        overview_open && occupied_right && (is_active || active_workspace_index == Some(index + 1));
    let left_inset = if overview_open && (is_active || split_left) {
        WORKSPACE_OVERVIEW_SPLIT_GAP / 2.
    } else {
        0.
    };
    let right_inset = if overview_open && (is_active || split_right) {
        WORKSPACE_OVERVIEW_SPLIT_GAP / 2.
    } else {
        0.
    };
    let size = WORKSPACE_SLOT_SIZE - left_inset - right_inset;
    let joined_left = occupied_left && !split_left;
    let joined_right = occupied_right && !split_right;

    OccupiedBackgroundGeometry {
        position: index as f32 * WORKSPACE_SLOT_SIZE + left_inset,
        size,
        start_radius: if joined_left { 0. } else { size / 2. },
        end_radius: if joined_right { 0. } else { size / 2. },
    }
}

/// Workspaces widget for status bar consuming compositor snapshots.
#[derive(IntoElement)]
pub struct WorkspacesWidget {
    id: ElementId,
    workspaces: Vec<WorkspaceInfo>,
    connection: CompositorConnection,
    orientation: PillOrientation,
    style: StyleRefinement,
}

impl WorkspacesWidget {
    pub fn new(
        id: impl Into<ElementId>,
        workspaces: Vec<WorkspaceInfo>,
        connection: CompositorConnection,
        orientation: PillOrientation,
    ) -> Self {
        Self {
            id: id.into(),
            workspaces,
            connection,
            orientation,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for WorkspacesWidget {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn render_workspace_dot(
    ws: &WorkspaceInfo,
    is_ready: bool,
    display_id: Option<gpui::DisplayId>,
    cx: &mut App,
) -> gpui::AnyElement {
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
                ShellRuntime::open_or_focus_overview_on_display(cx, display_id);
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
        let display_id = window.display(cx).map(|display| display.id());
        let overview_open = ShellRuntime::is_overview_open(cx);

        let mut items: Vec<gpui::AnyElement> = Vec::new();
        let mut occupied_backgrounds: Vec<gpui::AnyElement> = Vec::new();
        let mut active_workspace_index: Option<usize> = None;

        if is_connecting {
            let badge = div()
                .id("ws_connecting")
                .role(Role::Status)
                .aria_label("Compositor connecting")
                .rounded_full()
                .bg(cx.theme().surface_container_highest.opacity(0.5))
                .text_color(cx.theme().on_surface_variant.opacity(0.6))
                .text_xs()
                .font_bold()
                .flex()
                .items_center()
                .justify_center();

            let badge = if self.orientation == PillOrientation::Horizontal {
                badge.h(px(24.)).px_2_5().child("Connecting...")
            } else {
                badge.w(px(24.)).py_2_5().child("...")
            };

            items.push(badge.into_any_element());
        } else if is_stopped {
            let last_err = match &self.connection {
                CompositorConnection::Stopped => "Compositor is unavailable".to_string(),
                _ => String::new(),
            };
            let badge = div()
                .id("ws_stopped")
                .role(Role::Status)
                .aria_label("Compositor unavailable")
                .tooltip(move |window, cx| Tooltip::new(last_err.clone()).build(window, cx))
                .rounded_full()
                .bg(cx.theme().error_container)
                .text_color(cx.theme().on_error_container)
                .text_xs()
                .font_bold()
                .flex()
                .items_center()
                .justify_center();

            let badge = if self.orientation == PillOrientation::Horizontal {
                badge.h(px(24.)).px_2_5().child("Compositor Unavailable")
            } else {
                badge.w(px(24.)).py_2_5().child("!")
            };

            items.push(badge.into_any_element());
        } else if is_ready && self.workspaces.is_empty() {
            let badge = div()
                .id("ws_empty")
                .role(Role::Status)
                .aria_label("No workspaces available")
                .rounded_full()
                .bg(cx.theme().surface_container_highest.opacity(0.5))
                .text_color(cx.theme().on_surface_variant.opacity(0.5))
                .text_xs()
                .font_bold()
                .flex()
                .items_center()
                .justify_center()
                .child("1");

            let badge = if self.orientation == PillOrientation::Horizontal {
                badge.h(px(24.)).min_w(px(26.)).px_2()
            } else {
                badge.w(px(24.)).min_h(px(26.)).py_2()
            };

            items.push(badge.into_any_element());
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

                let geometry = occupied_background_geometry(
                    index,
                    &occupied,
                    overview_open,
                    active_workspace_index,
                );

                if self.orientation == PillOrientation::Horizontal {
                    occupied_backgrounds.push(
                        div()
                            .absolute()
                            .top(px(0.))
                            .left(px(geometry.position))
                            .w(px(geometry.size))
                            .h(px(WORKSPACE_SLOT_SIZE))
                            .rounded_tl(px(geometry.start_radius))
                            .rounded_bl(px(geometry.start_radius))
                            .rounded_tr(px(geometry.end_radius))
                            .rounded_br(px(geometry.end_radius))
                            .bg(cx.theme().secondary_container.opacity(0.6))
                            .into_any_element(),
                    );
                } else {
                    occupied_backgrounds.push(
                        div()
                            .absolute()
                            .left(px(0.))
                            .top(px(geometry.position))
                            .w(px(WORKSPACE_SLOT_SIZE))
                            .h(px(geometry.size))
                            .rounded_tl(px(geometry.start_radius))
                            .rounded_tr(px(geometry.start_radius))
                            .rounded_bl(px(geometry.end_radius))
                            .rounded_br(px(geometry.end_radius))
                            .bg(cx.theme().secondary_container.opacity(0.6))
                            .into_any_element(),
                    );
                }
            }

            items.extend(
                self.workspaces
                    .iter()
                    .map(|ws| render_workspace_dot(ws, is_ready, display_id, cx)),
            );
        }

        let active_indicator_element =
            crate::bar::widgets::pill_strip::render_active_pill_indicator(
                &self.id,
                active_workspace_index,
                self.orientation,
                false,
                window,
                cx,
            );

        if let CompositorConnection::Reconnecting {
            attempt,
            ref last_error,
        } = self.connection
        {
            let error_msg = last_error
                .clone()
                .unwrap_or_else(|| "Attempting to reconnect to compositor...".into());
            let badge = div()
                .id("ws_reconnect_indicator")
                .role(Role::Status)
                .aria_label(format!("Compositor reconnecting attempt {}", attempt))
                .tooltip(move |window, cx| Tooltip::new(error_msg.clone()).build(window, cx))
                .flex()
                .items_center()
                .rounded_full()
                .bg(cx.theme().tertiary_container)
                .text_color(cx.theme().on_tertiary_container)
                .text_xs()
                .child(Icon::new(IconName::Info).size(px(12.)));

            let badge = if self.orientation == PillOrientation::Horizontal {
                badge
                    .ml_1()
                    .gap_1()
                    .px_2()
                    .h(px(24.))
                    .child(format!("Reconnecting ({})", attempt))
            } else {
                badge.mt_1().py_1().px_1().w(px(24.)).justify_center()
            };

            items.push(badge.into_any_element());
        }

        let items_layout = if self.orientation == PillOrientation::Horizontal {
            h_flex()
                .items_center()
                .gap(px(0.))
                .children(items)
                .into_any_element()
        } else {
            v_flex()
                .items_center()
                .gap(px(0.))
                .children(items)
                .into_any_element()
        };

        let container = div()
            .id(self.id)
            .relative()
            .flex()
            .items_center()
            .children(occupied_backgrounds)
            .child(active_indicator_element)
            .child(items_layout);

        if self.orientation == PillOrientation::Horizontal {
            container
                .flex_row()
                .h(px(WORKSPACE_SLOT_SIZE))
                .into_any_element()
        } else {
            container
                .flex_col()
                .w(px(WORKSPACE_SLOT_SIZE))
                .into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::widgets::pill_strip::{
        PILL_INDICATOR_SIZE, calculate_stretching_geometry, indicator_target,
    };
    use gpui::Pixels;

    fn as_f32(value: Pixels) -> f32 {
        value.into()
    }

    #[test]
    fn workspace_geometry_matches_inir_slot_grid() {
        let first = indicator_target(0);
        let third = indicator_target(2);

        assert_eq!(as_f32(first.position), 2.);
        assert_eq!(as_f32(first.size), 22.);
        assert_eq!(as_f32(third.position), 54.);
    }

    #[test]
    fn workspace_indicator_stretches_then_settles_on_target() {
        let from = indicator_target(0);
        let target = indicator_target(2);
        let halfway = calculate_stretching_geometry(from, target, 0.5);
        let settled = calculate_stretching_geometry(from, target, 1.);

        assert!(as_f32(halfway.size) > PILL_INDICATOR_SIZE);
        assert!(as_f32(halfway.position) > as_f32(from.position));
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

    #[test]
    fn overview_only_splits_the_active_workspace_from_occupied_groups() {
        let occupied = [true, true, true, true, false];
        let joined_middle = occupied_background_geometry(1, &occupied, false, Some(1));
        assert_eq!(joined_middle.position, WORKSPACE_SLOT_SIZE);
        assert_eq!(joined_middle.size, WORKSPACE_SLOT_SIZE);
        assert_eq!(joined_middle.start_radius, 0.);
        assert_eq!(joined_middle.end_radius, 0.);

        let split_middle = occupied_background_geometry(1, &occupied, true, Some(1));
        assert_eq!(
            split_middle.position,
            WORKSPACE_SLOT_SIZE + WORKSPACE_OVERVIEW_SPLIT_GAP / 2.
        );
        assert_eq!(
            split_middle.size,
            WORKSPACE_SLOT_SIZE - WORKSPACE_OVERVIEW_SPLIT_GAP
        );
        assert_eq!(split_middle.start_radius, split_middle.size / 2.);
        assert_eq!(split_middle.end_radius, split_middle.size / 2.);

        let grouped_after_active = occupied_background_geometry(2, &occupied, true, Some(1));
        assert_eq!(
            grouped_after_active.position,
            2. * WORKSPACE_SLOT_SIZE + WORKSPACE_OVERVIEW_SPLIT_GAP / 2.
        );
        assert_eq!(
            grouped_after_active.size,
            WORKSPACE_SLOT_SIZE - WORKSPACE_OVERVIEW_SPLIT_GAP / 2.
        );
        assert_eq!(
            grouped_after_active.start_radius,
            grouped_after_active.size / 2.
        );
        assert_eq!(grouped_after_active.end_radius, 0.);

        let grouped_tail = occupied_background_geometry(3, &occupied, true, Some(1));
        assert_eq!(grouped_tail.position, 3. * WORKSPACE_SLOT_SIZE);
        assert_eq!(grouped_tail.size, WORKSPACE_SLOT_SIZE);
        assert_eq!(grouped_tail.start_radius, 0.);
        assert_eq!(grouped_tail.end_radius, WORKSPACE_SLOT_SIZE / 2.);
    }

    #[test]
    fn workspace_vertical_orientation_geometry() {
        let occupied = [true, false, true];
        let geom_0 = occupied_background_geometry(0, &occupied, false, Some(0));
        let geom_2 = occupied_background_geometry(2, &occupied, false, Some(0));

        assert_eq!(geom_0.position, 0.);
        assert_eq!(geom_0.size, WORKSPACE_SLOT_SIZE);
        assert_eq!(geom_2.position, 2. * WORKSPACE_SLOT_SIZE);
        assert_eq!(geom_2.size, WORKSPACE_SLOT_SIZE);
    }
}
