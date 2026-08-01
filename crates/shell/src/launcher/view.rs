use crate::actions::{ActionDescriptor, ActionInvocation};
use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, Role, StatefulInteractiveElement, Styled, Window, div,
    img, px,
};
use shilpo_services::{AppScanner, Application};
use shilpo_ui::{
    ActiveTheme, Colorize, ContextMenuExt, FocusTrapElement, Icon, IconName, PopupMenuItem,
    Sizable, StyledExt, h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};
use std::{sync::mpsc::TryRecvError, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherCategory {
    All,
    Development,
    System,
    Utility,
    Media,
}

impl LauncherCategory {
    pub const ALL: &[Self] = &[
        Self::All,
        Self::Development,
        Self::System,
        Self::Utility,
        Self::Media,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Development => "Dev",
            Self::System => "System",
            Self::Utility => "Utility",
            Self::Media => "Media",
        }
    }

    /// Returns true if this application matches the category filter.
    pub fn matches(self, app: &Application) -> bool {
        match self {
            Self::All => true,
            Self::Development => app.categories.iter().any(|c| {
                matches!(
                    c.as_str(),
                    "Development" | "IDE" | "TextEditor" | "WebBrowser"
                )
            }),
            Self::System => app.categories.iter().any(|c| {
                matches!(
                    c.as_str(),
                    "System" | "Settings" | "Monitor" | "TerminalEmulator" | "PackageManager"
                )
            }),
            Self::Utility => app
                .categories
                .iter()
                .any(|c| matches!(c.as_str(), "Utility" | "Accessibility" | "FileManager")),
            Self::Media => app.categories.iter().any(|c| {
                matches!(
                    c.as_str(),
                    "Audio"
                        | "Video"
                        | "AudioVideo"
                        | "Music"
                        | "Player"
                        | "Graphics"
                        | "Photography"
                )
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub enum LauncherSearchResult {
    App(Application),
    Action(ActionDescriptor),
    FilePath(std::path::PathBuf),
    Uri(String),
}

fn search_results(
    scanner: &AppScanner,
    text: &str,
    category: LauncherCategory,
    recent_apps: &[String],
    actions: &[ActionDescriptor],
) -> Vec<LauncherSearchResult> {
    let query = text.trim().to_lowercase();
    let mut apps: Vec<Application> = scanner
        .search(text)
        .into_iter()
        .filter(|app| category.matches(app))
        .collect();

    apps.sort_by_key(|app| {
        recent_apps
            .iter()
            .position(|id| id == &app.exec)
            .unwrap_or(usize::MAX)
    });

    let mut results = Vec::new();
    if let Some(path) = expand_path(text) {
        results.push(LauncherSearchResult::FilePath(path));
    } else if is_uri_spec(text) {
        results.push(LauncherSearchResult::Uri(text.trim().to_string()));
    }
    results.extend(apps.into_iter().map(LauncherSearchResult::App));
    results.extend(
        actions
            .iter()
            .filter(|action| {
                action.input.can_invoke_without_input()
                    && (query.is_empty()
                        || action.label.to_lowercase().contains(&query)
                        || action.name.contains(&query))
            })
            .cloned()
            .map(LauncherSearchResult::Action),
    );
    results
}

fn expand_path(query: &str) -> Option<std::path::PathBuf> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let expanded = if let Some(stripped) = q.strip_prefix("~/") {
        std::env::var("HOME")
            .ok()
            .map(|h| std::path::PathBuf::from(h).join(stripped))
    } else if q.starts_with('/') {
        Some(std::path::PathBuf::from(q))
    } else {
        None
    };

    if let Some(path) = expanded
        && path.exists()
    {
        Some(path)
    } else {
        None
    }
}

fn is_uri_spec(query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.starts_with("http://")
        || q.starts_with("https://")
        || q.starts_with("file://")
        || q.starts_with("mailto:")
        || q.starts_with("ssh://")
        || q.starts_with("ftp://")
}

/// M3 Expressive Application Launcher Overlay View powered by shilpo-ui's Input element.
pub struct LauncherView {
    pub scanner: AppScanner,
    input_state: Entity<InputState>,
    results: Vec<LauncherSearchResult>,
    selected_index: usize,
    active_category: LauncherCategory,
    pub loading: bool,
    _catalog_task: gpui::Task<()>,
}

impl LauncherView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let scanner = ShellRuntime::app_scanner(cx).unwrap_or_else(AppScanner::new_empty);
        let catalog_updates = scanner.subscribe();
        let loading = scanner.applications().is_empty();
        let results = search_results(
            &scanner,
            "",
            LauncherCategory::All,
            &ShellRuntime::recent_apps(cx),
            &ShellRuntime::action_descriptors(cx),
        );

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

        let catalog_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                match catalog_updates.try_recv() {
                    Ok(()) => {
                        while catalog_updates.try_recv().is_ok() {}
                        if this
                            .update(cx, |view, cx| {
                                view.loading = false;
                                view.update_search(cx);
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => return,
                }
            }
        });

        // Keep the launcher attached to the shared catalog while filling an empty cache.
        if loading {
            let scanner_clone = scanner.clone();
            cx.spawn(async move |_, cx| {
                cx.background_executor()
                    .spawn(async move { scanner_clone.rescan() })
                    .await;
            })
            .detach();
        }

        Self {
            scanner,
            input_state,
            results,
            selected_index: 0,
            active_category: LauncherCategory::All,
            loading,
            _catalog_task: catalog_task,
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
        for descriptor in ShellRuntime::extension_descriptors(
            cx,
            crate::extensions::ContributionSurface::Launcher,
        ) {
            ShellRuntime::dispatch_extension_input(
                cx,
                &descriptor.id,
                None,
                "query",
                Some(text.clone().into()),
            );
        }
        let recent_apps = ShellRuntime::recent_apps(cx);
        self.results = search_results(
            &self.scanner,
            &text,
            self.active_category,
            &recent_apps,
            &ShellRuntime::action_descriptors(cx),
        );
        self.selected_index = 0;
        cx.notify();
    }

    fn set_category(&mut self, category: LauncherCategory, cx: &mut Context<Self>) {
        self.active_category = category;
        self.update_search(cx);
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
                    ShellRuntime::record_recent_app(cx, &app.exec);
                    app.launch_with_feedback(|err_msg| {
                        tracing::warn!(error = %err_msg, "application launch failed");
                    });
                    ShellRuntime::forget_launcher(cx);
                    window.remove_window();
                }
                LauncherSearchResult::Action(action) => {
                    match ActionInvocation::from_id_and_payload(action.id.clone(), None) {
                        Ok(invocation) => {
                            if let Ok(()) = ShellRuntime::dispatch_action(cx, invocation) {
                                ShellRuntime::forget_launcher(cx);
                                window.remove_window();
                            }
                        }
                        Err(err) => {
                            tracing::warn!(
                                action_id = %action.id,
                                error = %err,
                                "launcher action dispatch failed"
                            );
                        }
                    }
                }
                LauncherSearchResult::FilePath(path) => {
                    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
                    ShellRuntime::forget_launcher(cx);
                    window.remove_window();
                }
                LauncherSearchResult::Uri(uri) => {
                    let _ = std::process::Command::new("xdg-open").arg(uri).spawn();
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

    fn cycle_category(&mut self, forward: bool, cx: &mut Context<Self>) {
        let all = LauncherCategory::ALL;
        let current_idx = all
            .iter()
            .position(|&c| c == self.active_category)
            .unwrap_or(0);
        let next_idx = if forward {
            (current_idx + 1) % all.len()
        } else {
            (current_idx + all.len() - 1) % all.len()
        };
        self.set_category(all[next_idx], cx);
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
            "tab" => {
                let shift = event.keystroke.modifiers.shift;
                self.cycle_category(!shift, cx);
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_idx = self.selected_index;
        let query_text = self.input_state.read(cx).value().to_string();
        let provider_views = ShellRuntime::extension_surface_views(
            cx,
            crate::extensions::ContributionSurface::Launcher,
        )
        .into_iter()
        .map(|(id, tree)| {
            crate::bar::ext_view_adapter::render_ext_view_tree(&id, None, &tree, window, cx)
        })
        .collect::<Vec<_>>();

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
                            .child(Icon::new(IconName::Terminal).size(px(20.)))
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
                                            .child(action.label.clone()),
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
                LauncherSearchResult::FilePath(path) => {
                    let file_icon = div()
                        .w(px(42.))
                        .h(px(42.))
                        .rounded_xl()
                        .bg(cx.theme().primary_container)
                        .text_color(cx.theme().on_primary_container)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::Folder).size(px(20.)));

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
                            .child(file_icon)
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_bold()
                                            .text_color(cx.theme().on_surface)
                                            .child(
                                                path.file_name()
                                                    .and_then(|n| n.to_str())
                                                    .unwrap_or("File")
                                                    .to_string(),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().on_surface_variant)
                                            .child(path.display().to_string()),
                                    ),
                            ),
                    )
                }
                LauncherSearchResult::Uri(uri) => {
                    let uri_icon = div()
                        .w(px(42.))
                        .h(px(42.))
                        .rounded_xl()
                        .bg(cx.theme().secondary_container)
                        .text_color(cx.theme().on_secondary_container)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(IconName::Star).size(px(20.)));

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
                            .child(uri_icon)
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_bold()
                                            .text_color(cx.theme().on_surface)
                                            .child("Open URI Link"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().on_surface_variant)
                                            .child(uri.clone()),
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
                                .child(Icon::new(IconName::Terminal).size(px(16.)))
                        };

                        let app_exec = app.exec.clone();
                        h_flex()
                            .id(("app-item", i))
                            .role(Role::Button)
                            .aria_label(app.name.clone())
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
                                        .icon(IconName::PlayArrow)
                                        .accelerator("Enter")
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
                                        .icon(IconName::ContentCopy)
                                        .accelerator("Ctrl+C")
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
                                    .child(
                                        div().text_sm().font_semibold().child(action.label.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().on_surface_variant)
                                            .child(format!("Action: {}", action.name)),
                                    ),
                            )
                            .into_any_element()
                    }
                    LauncherSearchResult::FilePath(path) => {
                        let file_icon = div()
                            .w(px(32.))
                            .h(px(32.))
                            .rounded_lg()
                            .bg(cx.theme().primary_container)
                            .text_color(cx.theme().on_primary_container)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Icon::new(IconName::Folder).size(px(16.)));

                        h_flex()
                            .id(("file-item", i))
                            .px_4()
                            .py_2()
                            .rounded_xl()
                            .bg(bg)
                            .text_color(fg)
                            .gap_3()
                            .items_center()
                            .child(file_icon)
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .child(
                                        div().text_sm().font_semibold().child(
                                            path.file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("File")
                                                .to_string(),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().on_surface_variant)
                                            .child(path.display().to_string()),
                                    ),
                            )
                            .into_any_element()
                    }
                    LauncherSearchResult::Uri(uri) => {
                        let uri_icon = div()
                            .w(px(32.))
                            .h(px(32.))
                            .rounded_lg()
                            .bg(cx.theme().secondary_container)
                            .text_color(cx.theme().on_secondary_container)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Icon::new(IconName::Star).size(px(16.)));

                        h_flex()
                            .id(("uri-item", i))
                            .px_4()
                            .py_2()
                            .rounded_xl()
                            .bg(bg)
                            .text_color(fg)
                            .gap_3()
                            .items_center()
                            .child(uri_icon)
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .child(div().text_sm().font_semibold().child("Open URI Link"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().on_surface_variant)
                                            .child(uri.clone()),
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
                        .child(Icon::new(IconName::Terminal).size(px(16.))),
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
                    .role(Role::Dialog)
                    .aria_label("Application Launcher")
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
                    // Category Filter Pills
                    .child(h_flex().gap_1p5().children(
                        LauncherCategory::ALL.iter().enumerate().map(|(i, &cat)| {
                            let is_active = self.active_category == cat;
                            let (bg, fg) = if is_active {
                                (cx.theme().primary, cx.theme().on_primary)
                            } else {
                                (
                                    cx.theme().surface_container_highest.opacity(0.6),
                                    cx.theme().on_surface_variant,
                                )
                            };

                            div()
                                .id(("cat-pill", i))
                                .role(Role::Button)
                                .aria_label(format!("Filter {}", cat.label()))
                                .px_3()
                                .py_1()
                                .rounded_full()
                                .bg(bg)
                                .text_color(fg)
                                .text_xs()
                                .font_semibold()
                                .cursor_pointer()
                                .hover(|s| {
                                    if is_active {
                                        s.bg(cx.theme().primary.darken(0.05))
                                    } else {
                                        s.bg(cx.theme().surface_container_highest)
                                    }
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_category(cat, cx);
                                }))
                                .child(cat.label())
                        }),
                    ))
                    // Prominent Top Result Card
                    .children(top_match)
                    // Secondary list results and fallback providers
                    .child(
                        v_flex()
                            .gap_1p5()
                            .children(other_items)
                            .children(provider_views)
                            .child(run_cmd_item)
                            .child(search_web_item),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn populated_catalog_produces_initial_launcher_results() {
        let app = Application {
            name: "Editor".into(),
            exec: "editor".into(),
            icon: None,
            icon_path: None,
            description: None,
            categories: vec!["Development".into()],
            desktop_file: "/tmp/editor.desktop".into(),
            working_dir: None,
            terminal: false,
            try_exec: None,
        };
        let scanner = AppScanner::from_applications(vec![app]);

        let results = search_results(
            &scanner,
            "",
            LauncherCategory::All,
            &[],
            &crate::actions::ActionRegistry::default().all(),
        );

        assert!(matches!(
            results.first(),
            Some(LauncherSearchResult::App(app)) if app.name == "Editor"
        ));
    }

    #[test]
    fn search_results_omits_parameterized_actions_and_includes_no_input_and_extension_actions() {
        let scanner = AppScanner::from_applications(vec![]);
        let mut registry = crate::actions::ActionRegistry::default();
        let canonical: shilpo_ext::CanonicalId =
            "io.github.alice.world-clock/refresh".parse().unwrap();
        let ext_id = registry
            .register_extension(canonical, "refresh", "Refresh World Clock")
            .unwrap();

        let results = search_results(&scanner, "", LauncherCategory::All, &[], &registry.all());

        let action_ids: Vec<crate::actions::ActionId> = results
            .into_iter()
            .filter_map(|r| match r {
                LauncherSearchResult::Action(a) => Some(a.id),
                _ => None,
            })
            .collect();

        // Parameterized actions must be absent
        assert!(!action_ids.contains(&crate::actions::ActionId::FocusWorkspace));
        assert!(!action_ids.contains(&crate::actions::ActionId::FocusWindow));
        assert!(!action_ids.contains(&crate::actions::ActionId::CloseWindow));
        assert!(!action_ids.contains(&crate::actions::ActionId::MoveWindowToWorkspace));

        // Representative no-input actions must be present
        assert!(action_ids.contains(&crate::actions::ActionId::ToggleLauncher));
        assert!(action_ids.contains(&crate::actions::ActionId::CreateWorkspace));

        // Registered extension action must be present
        assert!(action_ids.contains(&ext_id));
    }
}
