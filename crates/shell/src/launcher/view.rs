use crate::actions::{ActionDescriptor, ActionRegistry};
use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, Styled, Window, div, img, px,
};
use shilpo_services::{AppScanner, Application};
use shilpo_ui::{
    ActiveTheme, ContextMenuExt, FocusTrapElement, Icon, IconName, PopupMenuItem, Sizable,
    StyledExt, h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};

#[derive(Debug, Clone)]
pub enum LauncherSearchResult {
    App(Application),
    Action(ActionDescriptor),
}

/// M3 Expressive Application Launcher Overlay View powered by shilpo-ui's Input element.
pub struct LauncherView {
    pub scanner: AppScanner,
    input_state: Entity<InputState>,
    results: Vec<LauncherSearchResult>,
    selected_index: usize,
    pub loading: bool,
}

impl LauncherView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let scanner = AppScanner::new_empty();

        let input_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search applications..."));

        cx.subscribe(&input_state, |this, _state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.update_search(cx);
            }
        })
        .detach();

        let focus_handle = input_state.read(cx).focus_handle(cx);
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

        // Spawn background scan task so the launcher opens instantly on frame 1
        cx.spawn(async move |this, cx| {
            let scanned = cx
                .background_executor()
                .spawn(async move {
                    let scanner = AppScanner::new().unwrap_or_default();
                    scanner.applications()
                })
                .await;

            let _ = this.update(cx, |view, cx| {
                view.scanner = AppScanner::from_applications(scanned);
                view.update_search(cx);
                view.loading = false;
                cx.notify();
            });
        })
        .detach();

        Self {
            scanner,
            input_state,
            results: Vec::new(),
            selected_index: 0,
            loading: true,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<shilpo_ui::Root> {
        let launcher = cx.new(|cx| Self::new(window, cx));
        cx.new(|cx| {
            shilpo_ui::Root::new(launcher, window, cx)
                .bordered(false)
                .bg(cx.theme().transparent)
        })
    }

    fn update_search(&mut self, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).value().to_string();
        let q = text.trim().to_lowercase();
        let mut combined = Vec::new();

        for app in self.scanner.search(&text) {
            combined.push(LauncherSearchResult::App(app));
        }

        for action in ActionRegistry::all() {
            if q.is_empty() || action.label.to_lowercase().contains(&q) || action.name.contains(&q)
            {
                combined.push(LauncherSearchResult::Action(action));
            }
        }

        self.results = combined;
        self.selected_index = 0;
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let total_items = self.results.len() + 2; // Results + 2 provider fallbacks
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
            match &self.results[self.selected_index] {
                LauncherSearchResult::App(app) => {
                    app.launch();
                    ShellRuntime::forget_launcher(cx);
                    window.remove_window();
                }
                LauncherSearchResult::Action(action) => {
                    let _ = ShellRuntime::dispatch_action(cx, action.id);
                    ShellRuntime::forget_launcher(cx);
                    window.remove_window();
                }
            }
        } else {
            // Launch terminal command or web search
            let query_val = self.input_state.read(cx).value().to_string();
            let cmd = query_val.trim();
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
            _ => {}
        }
    }
}

impl Focusable for LauncherView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input_state.read(cx).focus_handle(cx)
    }
}

impl Render for LauncherView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_idx = self.selected_index;
        let query_text = self.input_state.read(cx).value().to_string();

        // Render Top Match (First match) prominent card
        let top_match = if let Some(item) = self.results.first() {
            let is_selected = selected_idx == 0;
            let bg = if is_selected {
                cx.theme().primary_container.opacity(0.24)
            } else {
                cx.theme().surface_container.opacity(0.4)
            };

            match item {
                LauncherSearchResult::App(app) => {
                    let app_icon = if let Some(path) = &app.icon_path {
                        div()
                            .w(px(42.))
                            .h(px(42.))
                            .flex()
                            .items_center()
                            .justify_center()
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
                            .id("top-match-card")
                            .px_4()
                            .py_3()
                            .rounded_2xl()
                            .bg(bg)
                            .border_1()
                            .border_color(if is_selected {
                                cx.theme().primary
                            } else {
                                cx.theme().outline_variant.opacity(0.3)
                            })
                            .gap_4()
                            .items_center()
                            .child(app_icon)
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_bold()
                                            .text_color(cx.theme().on_surface)
                                            .child(app.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().on_surface_variant)
                                            .child(
                                                app.description
                                                    .clone()
                                                    .unwrap_or_else(|| app.exec.clone()),
                                            ),
                                    ),
                            ),
                    )
                }
                LauncherSearchResult::Action(action) => {
                    let action_icon = div()
                        .w(px(42.))
                        .h(px(42.))
                        .rounded_xl()
                        .bg(cx.theme().secondary)
                        .text_color(cx.theme().on_secondary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::Settings).size(px(20.)));

                    Some(
                        h_flex()
                            .id("top-match-card")
                            .px_4()
                            .py_3()
                            .rounded_2xl()
                            .bg(bg)
                            .border_1()
                            .border_color(if is_selected {
                                cx.theme().secondary
                            } else {
                                cx.theme().outline_variant.opacity(0.3)
                            })
                            .gap_4()
                            .items_center()
                            .child(action_icon)
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_bold()
                                            .text_color(cx.theme().on_surface)
                                            .child(action.label),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().on_surface_variant)
                                            .child(format!("System Action: {}", action.name)),
                                    ),
                            ),
                    )
                }
            }
        } else {
            None
        };

        // Render secondary result items
        let other_items = self
            .results
            .iter()
            .enumerate()
            .skip(1)
            .take(5)
            .map(|(i, item)| {
                let is_selected = selected_idx == i;
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

                match item {
                    LauncherSearchResult::App(app) => {
                        let app_icon = if let Some(path) = &app.icon_path {
                            div()
                                .w(px(32.))
                                .h(px(32.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(img(path.clone()).w(px(24.)).h(px(24.)))
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

                        let app_exec = app.exec.clone();
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
                            .context_menu(move |menu, _window, _cx| {
                                let exec_str = app_exec.clone();
                                let exec_launch = app_exec.clone();
                                menu.item(
                                    PopupMenuItem::new("Launch Application")
                                        .icon(IconName::Play)
                                        .on_click(move |_, window, cx| {
                                            let _ = std::process::Command::new("sh")
                                                .arg("-c")
                                                .arg(&exec_launch)
                                                .spawn();
                                            ShellRuntime::forget_launcher(cx);
                                            window.remove_window();
                                        }),
                                )
                                .item(
                                    PopupMenuItem::new("Copy Exec Command")
                                        .icon(IconName::Copy)
                                        .on_click(move |_, _window, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                exec_str.clone(),
                                            ));
                                        }),
                                )
                            })
                            .into_any_element()
                    }
                    LauncherSearchResult::Action(action) => {
                        let action_icon = div()
                            .w(px(32.))
                            .h(px(32.))
                            .rounded_lg()
                            .bg(cx.theme().secondary)
                            .text_color(cx.theme().on_secondary)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Icon::new(IconName::Settings).size(px(16.)));

                        h_flex()
                            .id(("action-item", i))
                            .px_4()
                            .py_2()
                            .rounded_xl()
                            .bg(bg)
                            .text_color(fg)
                            .gap_3()
                            .items_center()
                            .child(action_icon)
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .child(div().text_sm().font_semibold().child(action.label))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().on_surface_variant)
                                            .child(format!("Action: {}", action.name)),
                                    ),
                            )
                            .into_any_element()
                    }
                }
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
                                .child(if query_text.is_empty() {
                                    "...".to_string()
                                } else {
                                    query_text.clone()
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
                                .child(if query_text.is_empty() {
                                    "...".to_string()
                                } else {
                                    query_text.clone()
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
                    .focus_trap("launcher-card-trap", &self.focus_handle(cx))
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
                    // Search Input Header powered by shilpo_ui::Input
                    .child(
                        h_flex()
                            .px_4()
                            .py_2()
                            .rounded_2xl()
                            .bg(cx.theme().surface_container_highest)
                            .justify_between()
                            .items_center()
                            .w_full()
                            .child(
                                Input::new(&self.input_state)
                                    .appearance(false)
                                    .cleanable(true)
                                    .prefix(Icon::new(IconName::Search).size(px(20.)))
                                    .with_size(shilpo_ui::Size::Medium),
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
