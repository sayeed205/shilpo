use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, Styled, Window, div, img, px,
};
use shilpo_services::{AppScanner, Application};
use shilpo_ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex, v_flex};

/// M3 Expressive Application Launcher Overlay View.
pub struct LauncherView {
    pub scanner: AppScanner,
    query: String,
    results: Vec<Application>,
    selected_index: usize,
    focus_handle: FocusHandle,
}

impl LauncherView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let scanner = AppScanner::new().unwrap();
        let results = scanner.applications();
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        window.on_window_should_close(cx, |_, cx| {
            ShellRuntime::forget_launcher(cx);
            true
        });

        // Dynamic theme synchronization with OS appearance
        shilpo_ui::Theme::sync_system_appearance(Some(window), cx);
        cx.observe_window_appearance(window, |_, window, cx| {
            shilpo_ui::Theme::sync_system_appearance(Some(window), cx);
            window.refresh();
        })
        .detach();

        Self {
            scanner,
            query: String::new(),
            results,
            selected_index: 0,
            focus_handle,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn update_search(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query;
        self.results = self.scanner.search(&self.query);
        self.selected_index = 0;
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let total_items = self.results.len() + 2; // Apps + 2 provider fallbacks
        if total_items == 0 {
            return;
        }
        let len = total_items as isize;
        let new_idx = (self.selected_index as isize + delta).rem_euclid(len) as usize;
        self.selected_index = new_idx;
        cx.notify();
    }

    fn launch_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_index < self.results.len() {
            if let Some(app) = self.results.get(self.selected_index) {
                app.launch();
                ShellRuntime::forget_launcher(cx);
                window.remove_window();
            }
        } else {
            // Launch terminal command or web search
            let cmd = self.query.trim();
            if !cmd.is_empty() {
                if self.selected_index == self.results.len() {
                    let _ = std::process::Command::new("sh").args(["-c", cmd]).spawn();
                } else {
                    let query = cmd.replace(' ', "+");
                    let url = format!("https://www.google.com/search?q={}", query);
                    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
                }
                ShellRuntime::forget_launcher(cx);
                window.remove_window();
            }
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => {
                ShellRuntime::forget_launcher(cx);
                window.remove_window();
            }
            "enter" => {
                self.launch_selected(window, cx);
            }
            "down" => {
                self.move_selection(1, cx);
            }
            "up" => {
                self.move_selection(-1, cx);
            }
            "backspace" => {
                let mut q = self.query.clone();
                q.pop();
                self.update_search(q, cx);
            }
            ch if ch.len() == 1 => {
                let mut q = self.query.clone();
                q.push_str(ch);
                self.update_search(q, cx);
            }
            _ => {}
        }
    }
}

impl Focusable for LauncherView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for LauncherView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_idx = self.selected_index;

        // Render Top Match (First match) prominent card
        let top_match = if let Some(app) = self.results.first() {
            let is_selected = selected_idx == 0;
            let bg = if is_selected {
                cx.theme().primary_container.opacity(0.24)
            } else {
                cx.theme().surface_container.opacity(0.4)
            };

            let app_icon = if let Some(path) = &app.icon_path {
                div()
                    .w(px(42.))
                    .h(px(42.))
                    .rounded_xl()
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(img(path.clone()).w(px(36.)).h(px(36.)))
            } else {
                div()
                    .w(px(42.))
                    .h(px(42.))
                    .rounded_xl()
                    .bg(cx.theme().primary)
                    .text_color(cx.theme().on_primary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::SquareTerminal).size(px(20.)))
            };

            Some(
                h_flex()
                    .id("top-match")
                    .px_5()
                    .py_4()
                    .rounded_2xl()
                    .bg(bg)
                    .border_1()
                    .border_color(cx.theme().outline_variant.opacity(0.3))
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex().gap_4().items_center().child(app_icon).child(
                            v_flex()
                                .gap_0p5()
                                .child(div().text_base().font_bold().child(app.name.clone()))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().on_surface_variant)
                                        .child(app.exec.clone()),
                                ),
                        ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .bg(cx.theme().primary)
                            .text_color(cx.theme().on_primary)
                            .text_xs()
                            .font_bold()
                            .child("Launch"),
                    ),
            )
        } else {
            None
        };

        // Render remaining list apps
        let other_items = self
            .results
            .iter()
            .enumerate()
            .skip(1)
            .take(3)
            .map(|(i, app)| {
                let is_selected = i == selected_idx;
                let (bg, fg) = if is_selected {
                    (
                        cx.theme().primary_container.opacity(0.18),
                        cx.theme().on_primary_container,
                    )
                } else {
                    (
                        cx.theme().surface_container.opacity(0.2),
                        cx.theme().on_surface,
                    )
                };

                let app_icon = if let Some(path) = &app.icon_path {
                    div()
                        .w(px(32.))
                        .h(px(32.))
                        .rounded_lg()
                        .flex()
                        .items_center()
                        .justify_center()
                        .overflow_hidden()
                        .child(img(path.clone()).w(px(26.)).h(px(26.)))
                } else {
                    div()
                        .w(px(32.))
                        .h(px(32.))
                        .rounded_lg()
                        .bg(cx.theme().primary)
                        .text_color(cx.theme().on_primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::SquareTerminal).size(px(16.)))
                };

                h_flex()
                    .id(("app-item", i))
                    .px_4()
                    .py_2()
                    .rounded_xl()
                    .bg(bg)
                    .text_color(fg)
                    .gap_3()
                    .items_center()
                    .child(app_icon)
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(div().text_sm().font_semibold().child(app.name.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().on_surface_variant)
                                    .child(app.exec.clone()),
                            ),
                    )
            });

        // Render provider fallbacks
        let run_cmd_idx = self.results.len();
        let search_web_idx = self.results.len() + 1;

        let run_cmd_item = {
            let is_selected = selected_idx == run_cmd_idx;
            let (bg, fg) = if is_selected {
                (
                    cx.theme().primary_container.opacity(0.18),
                    cx.theme().on_primary_container,
                )
            } else {
                (
                    cx.theme().surface_container.opacity(0.2),
                    cx.theme().on_surface,
                )
            };

            h_flex()
                .id("run-command-item")
                .px_4()
                .py_2()
                .rounded_xl()
                .bg(bg)
                .text_color(fg)
                .gap_3()
                .items_center()
                .child(
                    div()
                        .w(px(32.))
                        .h(px(32.))
                        .rounded_lg()
                        .bg(cx.theme().secondary_container)
                        .text_color(cx.theme().on_secondary_container)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::SquareTerminal).size(px(16.))),
                )
                .child(
                    v_flex()
                        .gap_0p5()
                        .child(div().text_sm().font_semibold().child("Run command"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().on_surface_variant)
                                .child(if self.query.is_empty() {
                                    "...".to_string()
                                } else {
                                    self.query.clone()
                                }),
                        ),
                )
        };

        let search_web_item = {
            let is_selected = selected_idx == search_web_idx;
            let (bg, fg) = if is_selected {
                (
                    cx.theme().primary_container.opacity(0.18),
                    cx.theme().on_primary_container,
                )
            } else {
                (
                    cx.theme().surface_container.opacity(0.2),
                    cx.theme().on_surface,
                )
            };

            h_flex()
                .id("search-web-item")
                .px_4()
                .py_2()
                .rounded_xl()
                .bg(bg)
                .text_color(fg)
                .gap_3()
                .items_center()
                .child(
                    div()
                        .w(px(32.))
                        .h(px(32.))
                        .rounded_lg()
                        .bg(cx.theme().secondary_container)
                        .text_color(cx.theme().on_secondary_container)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::Star).size(px(16.))),
                )
                .child(
                    v_flex()
                        .gap_0p5()
                        .child(div().text_sm().font_semibold().child("Search the web"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().on_surface_variant)
                                .child(if self.query.is_empty() {
                                    "...".to_string()
                                } else {
                                    self.query.clone()
                                }),
                        ),
                )
        };

        div()
            .w_full()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().scrim.opacity(0.4))
            .id("launcher-backdrop")
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                ShellRuntime::forget_launcher(cx);
                window.remove_window();
            })
            .child(
                v_flex()
                    .id("launcher-card")
                    .track_focus(&self.focus_handle(cx))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.handle_key_down(event, window, cx);
                    }))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .w(px(540.))
                    .p_4()
                    .gap_3()
                    .rounded_3xl()
                    .bg(cx.theme().surface_container_high)
                    .border_1()
                    .border_color(cx.theme().outline_variant.opacity(0.4))
                    .shadow_2xl()
                    // Search Input Header
                    .child(
                        h_flex()
                            .px_4()
                            .py_3()
                            .rounded_2xl()
                            .bg(cx.theme().surface_container_highest)
                            .justify_between()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap_3()
                                    .items_center()
                                    .child(Icon::new(IconName::Search).size(px(20.)))
                                    .child(
                                        div()
                                            .text_base()
                                            .font_semibold()
                                            .text_color(if self.query.is_empty() {
                                                cx.theme().on_surface_variant
                                            } else {
                                                cx.theme().on_surface
                                            })
                                            .child(if self.query.is_empty() {
                                                "Search applications...".to_string()
                                            } else {
                                                self.query.clone()
                                            }),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Icon::new(IconName::Copy).size(px(18.)))
                                    .child(Icon::new(IconName::Network).size(px(18.))),
                            ),
                    )
                    // Prominent Top Result Card
                    .children(top_match)
                    // Secondary list results and fallback providers
                    .child(
                        v_flex()
                            .gap_1p5()
                            .children(other_items)
                            .child(run_cmd_item)
                            .child(search_web_item),
                    ),
            )
    }
}
