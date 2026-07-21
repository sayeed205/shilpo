use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window,
};

use shilpo_ui::{
    ActiveTheme as _, Icon, IconName, Selectable as _, Sizable, Size,
    button::{Button, ButtonGroup, ButtonVariants as _, IconButton, IconButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    tab::{Tab, TabBar},
    v_flex,
};

use crate::section;

pub struct TabsStory {
    focus_handle: FocusHandle,
    active_tab_ix: usize,
    dynamic_active_tab_ix: usize,
    dynamic_tabs: Vec<usize>,
    dynamic_next_tab_id: usize,
    size: Size,
    menu: bool,
}

impl super::Story for TabsStory {
    fn title() -> &'static str {
        "Tabs"
    }

    fn description() -> &'static str {
        "A set of layered sections of content—known as tab panels—that are displayed one at a time."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }
}

impl TabsStory {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            active_tab_ix: 0,
            dynamic_active_tab_ix: 0,
            dynamic_tabs: vec![0, 1, 2],
            dynamic_next_tab_id: 3,
            size: Size::default(),
            menu: false,
        }
    }

    fn set_active_tab(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Self>) {
        self.active_tab_ix = ix;
        cx.notify();
    }

    fn set_size(&mut self, size: Size, _: &mut Window, cx: &mut Context<Self>) {
        self.size = size;
        cx.notify();
    }

    fn set_dynamic_active_tab(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Self>) {
        self.dynamic_active_tab_ix = ix;
        cx.notify();
    }

    fn add_dynamic_tab(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let id = self.dynamic_next_tab_id;
        self.dynamic_next_tab_id += 1;
        self.dynamic_tabs.push(id);
        self.dynamic_active_tab_ix = self.dynamic_tabs.len() - 1;
        cx.notify();
    }

    fn remove_last_dynamic_tab(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.dynamic_tabs.len() <= 1 {
            return;
        }

        self.dynamic_tabs.pop();
        if self.dynamic_active_tab_ix >= self.dynamic_tabs.len() {
            self.dynamic_active_tab_ix = self.dynamic_tabs.len() - 1;
        }
        cx.notify();
    }

    fn remove_dynamic_tab_at_index(
        &mut self,
        remove_ix: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dynamic_tabs.len() <= 1 || remove_ix >= self.dynamic_tabs.len() {
            return;
        }

        self.dynamic_tabs.remove(remove_ix);
        if self.dynamic_active_tab_ix >= self.dynamic_tabs.len() {
            self.dynamic_active_tab_ix = self.dynamic_tabs.len() - 1;
        }
        cx.notify();
    }
}

impl Focusable for TabsStory {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TabsStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        ButtonGroup::new("toggle-size")
                            .outline()
                            .compact()
                            .child(
                                Button::new("xsmall")
                                    .label("XSmall")
                                    .selected(self.size == Size::XSmall),
                            )
                            .child(
                                Button::new("small")
                                    .label("Small")
                                    .selected(self.size == Size::Small),
                            )
                            .child(
                                Button::new("medium")
                                    .label("Medium")
                                    .selected(self.size == Size::Medium),
                            )
                            .child(
                                Button::new("large")
                                    .label("Large")
                                    .selected(self.size == Size::Large),
                            )
                            .on_click(cx.listener(|this, selecteds: &Vec<usize>, window, cx| {
                                let size = match selecteds[0] {
                                    0 => Size::XSmall,
                                    1 => Size::Small,
                                    2 => Size::Medium,
                                    3 => Size::Large,
                                    _ => unreachable!(),
                                };
                                this.set_size(size, window, cx);
                            })),
                    )
                    .child(
                        Checkbox::new("show-menu")
                            .label("More menu")
                            .checked(self.menu)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.menu = !this.menu;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                section("Tabs").max_w_md().child(
                    TabBar::new("tabs")
                        .w_full()
                        .with_size(self.size)
                        .menu(self.menu)
                        .selected_index(self.active_tab_ix)
                        .on_click(cx.listener(|this, ix: &usize, window, cx| {
                            this.set_active_tab(*ix, window, cx);
                        }))
                        .border_t_1()
                        .border_color(cx.theme().outline)
                        .prefix(
                            h_flex()
                                .mx_1()
                                .child(
                                    Button::new("back")
                                        .text()
                                        .xsmall()
                                        .icon(IconName::ArrowLeft),
                                )
                                .child(
                                    Button::new("forward")
                                        .text()
                                        .xsmall()
                                        .icon(IconName::ArrowRight),
                                ),
                        )
                        .child(Tab::new().label("Account"))
                        .child(Tab::new().label("Profile").disabled(true))
                        .child(Tab::new().label("Documents"))
                        .child(Tab::new().label("Mail"))
                        .child(Tab::new().label("Appearance"))
                        .child(Tab::new().label("Settings"))
                        .child(Tab::new().label("About"))
                        .child(Tab::new().label("License"))
                        .suffix(
                            h_flex()
                                .mx_1()
                                .child(Button::new("inbox").text().xsmall().icon(IconName::Inbox))
                                .child(
                                    Button::new("more").text().xsmall().icon(IconName::Ellipsis),
                                ),
                        ),
                ),
            )
            .child(
                section("Material 3 Primary Tabs").max_w_md().child(
                    TabBar::new("m3-primary")
                        .w_full()
                        .primary_tab()
                        .with_size(self.size)
                        .menu(self.menu)
                        .selected_index(self.active_tab_ix)
                        .on_click(cx.listener(|this, ix: &usize, window, cx| {
                            this.set_active_tab(*ix, window, cx);
                        }))
                        .child(
                            Tab::new()
                                .label("Mail")
                                .icon(IconName::Inbox)
                                .selected_icon(IconName::Star)
                                .badge_count(12),
                        )
                        .child(
                            Tab::new()
                                .label("Calendar")
                                .icon(IconName::Calendar)
                                .badge_count(3),
                        )
                        .child(Tab::new().label("Documents").icon(IconName::BookOpen))
                        .child(Tab::new().label("Settings").icon(IconName::Settings)),
                ),
            )
            .child(
                section("Material 3 Secondary Tabs").max_w_md().child(
                    TabBar::new("m3-secondary")
                        .w_full()
                        .secondary_tab()
                        .with_size(self.size)
                        .menu(self.menu)
                        .selected_index(self.active_tab_ix)
                        .on_click(cx.listener(|this, ix: &usize, window, cx| {
                            this.set_active_tab(*ix, window, cx);
                        }))
                        .child(Tab::new().label("Overview"))
                        .child(Tab::new().label("Specifications"))
                        .child(Tab::new().label("Reviews"))
                        .child(Tab::new().label("Support")),
                ),
            )
            .child(
                section("Material 3 Stacked Tabs").max_w_md().child(
                    TabBar::new("m3-stacked")
                        .w_full()
                        .primary_tab()
                        .large()
                        .selected_index(self.active_tab_ix)
                        .on_click(cx.listener(|this, ix: &usize, window, cx| {
                            this.set_active_tab(*ix, window, cx);
                        }))
                        .child(
                            Tab::new()
                                .label("Inbox")
                                .icon(IconName::Inbox)
                                .stacked(true),
                        )
                        .child(
                            Tab::new()
                                .label("Search")
                                .icon(IconName::Search)
                                .stacked(true),
                        )
                        .child(
                            Tab::new()
                                .label("Library")
                                .icon(IconName::BookOpen)
                                .badge_count(5)
                                .stacked(true),
                        ),
                ),
            )
            .child(
                section("Underline Tabs").max_w_md().child(
                    TabBar::new("underline")
                        .w_full()
                        .underline()
                        .with_size(self.size)
                        .menu(self.menu)
                        .selected_index(self.active_tab_ix)
                        .on_click(cx.listener(|this, ix: &usize, window, cx| {
                            this.set_active_tab(*ix, window, cx);
                        }))
                        .child("Account")
                        .child("Profile")
                        .child("Documents")
                        .child("Mail")
                        .child("Appearance")
                        .child("Settings")
                        .child("About")
                        .child("License"),
                ),
            )
            .child(
                section("Pill Tabs").max_w_md().child(
                    TabBar::new("pill")
                        .w_full()
                        .pill()
                        .with_size(self.size)
                        .menu(self.menu)
                        .selected_index(self.active_tab_ix)
                        .on_click(cx.listener(|this, ix: &usize, window, cx| {
                            this.set_active_tab(*ix, window, cx);
                        }))
                        .child(Tab::new().label("Account"))
                        .child(Tab::new().label("Profile").disabled(true))
                        .child(Tab::new().label("Documents & Files"))
                        .child(Tab::new().label("Mail"))
                        .child(Tab::new().label("Appearance"))
                        .child(Tab::new().label("Settings"))
                        .child(Tab::new().label("About"))
                        .child(Tab::new().label("License")),
                ),
            )
            .child(
                section("Outline Tabs").max_w_md().child(
                    TabBar::new("outline")
                        .w_full()
                        .outline()
                        .with_size(self.size)
                        .menu(self.menu)
                        .selected_index(self.active_tab_ix)
                        .on_click(cx.listener(|this, ix: &usize, window, cx| {
                            this.set_active_tab(*ix, window, cx);
                        }))
                        .child(Tab::new().label("Account"))
                        .child(Tab::new().label("Profile").disabled(true))
                        .child(Tab::new().label("Documents & Files"))
                        .child(Tab::new().label("Mail"))
                        .child(Tab::new().label("Appearance"))
                        .child(Tab::new().label("Settings"))
                        .child(Tab::new().label("About"))
                        .child(Tab::new().label("License")),
                ),
            )
            .child(
                section("Segmented Tabs").max_w_md().child(
                    TabBar::new("segmented")
                        .w_full()
                        .segmented()
                        .with_size(self.size)
                        .menu(self.menu)
                        .selected_index(self.active_tab_ix)
                        .on_click(cx.listener(|this, ix: &usize, window, cx| {
                            this.set_active_tab(*ix, window, cx);
                        }))
                        .child(IconName::Bot)
                        .child(IconName::Calendar)
                        .child(IconName::Map)
                        .children(vec!["Appearance", "Settings", "About", "License"]),
                ),
            )
            .child(
                section("Segmented Tabs (Dynamic with suffix and prefix)")
                    .max_w_md()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("add-tab")
                                    .outline()
                                    .compact()
                                    .label("Add Tab")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add_dynamic_tab(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("remove-tab")
                                    .outline()
                                    .compact()
                                    .label("Remove Last")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.remove_last_dynamic_tab(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        TabBar::new("segmented-dynamic")
                            .w_full()
                            .segmented()
                            .with_size(self.size)
                            .selected_index(self.dynamic_active_tab_ix)
                            .on_click(cx.listener(|this, ix: &usize, window, cx| {
                                this.set_dynamic_active_tab(*ix, window, cx);
                            }))
                            .children(self.dynamic_tabs.iter().enumerate().map(|(ix, id)| {
                                let label = format!("Tab {id}");
                                Tab::new()
                                    .px_2()
                                    .prefix(Icon::new(IconName::BookOpen))
                                    .label(label)
                                    .suffix(
                                        IconButton::new(format!("dynamic-tab-close-{id}"))
                                            .icon(IconName::Close)
                                            .standard()
                                            .xxsmall()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.remove_dynamic_tab_at_index(ix, window, cx);
                                            })),
                                    )
                                    .selected(self.dynamic_active_tab_ix == ix)
                            })),
                    ),
            )
            .child(
                section("Segmented Tabs (With filling space)")
                    .max_w_md()
                    .child(
                        TabBar::new("flex tabs")
                            .w_full()
                            .segmented()
                            .with_size(self.size)
                            .selected_index(self.active_tab_ix)
                            .on_click(cx.listener(|this, ix: &usize, window, cx| {
                                this.set_active_tab(*ix, window, cx);
                            }))
                            .child(Tab::new().flex_1().label("About"))
                            .child(Tab::new().flex_1().label("Profile")),
                    ),
            )
    }
}
