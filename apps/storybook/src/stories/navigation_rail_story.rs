use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window, div, px,
};
use shilpo_ui::{
    IconName, NavigationRail, NavigationRailHeader, NavigationRailItem, Selectable, StyledExt,
    badge::Badge, button::IconButton, h_flex, v_flex,
};

use crate::section;

pub struct NavigationRailStory {
    focus_handle: FocusHandle,
    selected_index: usize,
    previous_selected_index: Option<usize>,
    collapsed: bool,
}

impl NavigationRailStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            selected_index: 0,
            previous_selected_index: None,
            collapsed: true,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for NavigationRailStory {
    fn title() -> &'static str {
        "NavigationRail (M3 Expressive)"
    }

    fn description() -> &'static str {
        "Material 3 Expressive Navigation Rail with compact collapsed (80px) and wide expanded (240px) destination layouts."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<shilpo_ui::dock::PanelControl> {
        None
    }
}

impl Focusable for NavigationRailStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NavigationRailStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let is_collapsed = self.collapsed;
        let selected_index = self.selected_index;

        let toggle_button = {
            let entity = entity.clone();
            let icon = if is_collapsed {
                IconName::Menu
            } else {
                IconName::MenuOpen
            };
            IconButton::new("rail-toggle")
                .icon(icon)
                .on_click(move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.collapsed = !this.collapsed;
                        cx.notify();
                    });
                })
        };

        let header = NavigationRailHeader::new("rail-header").child(toggle_button);

        let items = vec![
            ("home", IconName::Star, "Home", false),
            ("search", IconName::Search, "Search", false),
            ("notifications", IconName::Notifications, "Updates", true),
            ("settings", IconName::Settings, "Settings", false),
        ];

        let rail_items: Vec<NavigationRailItem> = items
            .into_iter()
            .enumerate()
            .map(|(idx, (id, icon, label, has_badge))| {
                let entity = entity.clone();
                let mut item = NavigationRailItem::new(id)
                    .icon(icon)
                    .label(label)
                    .selected(selected_index == idx)
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |this, cx| {
                            this.previous_selected_index = Some(this.selected_index);
                            this.selected_index = idx;
                            cx.notify();
                        });
                    });

                if has_badge {
                    item = item.badge(Badge::new().count(3));
                }

                item
            })
            .collect();

        let rail = NavigationRail::new("story-rail")
            .collapsed(is_collapsed)
            .previous_selected_index(self.previous_selected_index)
            .header(header)
            .items(rail_items);

        v_flex().gap_6().child(
            section("Material 3 Expressive Navigation Rail")
                .max_w_2xl()
                .child(
                    h_flex().h(px(480.)).gap_4().child(rail).child(
                        v_flex()
                            .flex_1()
                            .p_6()
                            .gap_3()
                            .child(div().text_lg().font_bold().child(format!(
                                "Selected Destination: {}",
                                match selected_index {
                                    0 => "Home",
                                    1 => "Search",
                                    2 => "Updates (3)",
                                    3 => "Settings",
                                    _ => "Unknown",
                                }
                            )))
                            .child(div().text_sm().child(format!(
                                "Rail Mode: {}",
                                if is_collapsed {
                                    "Collapsed (80px Compact)"
                                } else {
                                    "Expanded (240px Wide)"
                                }
                            ))),
                    ),
                ),
        )
    }
}
