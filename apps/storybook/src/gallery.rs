use gpui::{prelude::*, *};
use shilpo_ui::{
    Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    resizable::{h_resizable, resizable_panel},
    separator::Separator,
    sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem},
    status_bar::StatusBar,
    v_flex,
};

use crate::*;

pub struct Gallery {
    stories: Vec<(&'static str, Vec<Entity<StoryContainer>>)>,
    active_group_index: Option<usize>,
    active_index: Option<usize>,
    collapsed: bool,
    search_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl Gallery {
    pub fn new(init_story: Option<&str>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
        let _subscriptions = vec![cx.subscribe(&search_input, |this, _, e, cx| match e {
            InputEvent::Change => {
                this.active_group_index = Some(0);
                this.active_index = Some(0);
                cx.notify()
            }
            _ => {}
        })];
        let stories = vec![
            (
                "Getting Started",
                vec![StoryContainer::panel::<WelcomeStory>(window, cx)],
            ),
            (
                "Components",
                vec![
                    StoryContainer::panel::<AccordionStory>(window, cx),
                    StoryContainer::panel::<AlertStory>(window, cx),
                    StoryContainer::panel::<AlertDialogStory>(window, cx),
                    StoryContainer::panel::<AvatarStory>(window, cx),
                    StoryContainer::panel::<BadgeStory>(window, cx),
                    StoryContainer::panel::<BreadcrumbStory>(window, cx),
                    StoryContainer::panel::<ButtonStory>(window, cx),
                    StoryContainer::panel::<ButtonGroupStory>(window, cx),
                    StoryContainer::panel::<CalendarStory>(window, cx),
                    StoryContainer::panel::<CardStory>(window, cx),
                    StoryContainer::panel::<CarouselStory>(window, cx),
                    StoryContainer::panel::<ChartStory>(window, cx),
                    StoryContainer::panel::<CheckboxStory>(window, cx),
                    StoryContainer::panel::<ChipStory>(window, cx),
                    StoryContainer::panel::<ClipboardStory>(window, cx),
                    StoryContainer::panel::<CollapsibleStory>(window, cx),
                    StoryContainer::panel::<ColorPickerStory>(window, cx),
                    StoryContainer::panel::<ComboboxStory>(window, cx),
                    StoryContainer::panel::<DatePickerStory>(window, cx),
                    StoryContainer::panel::<DescriptionListStory>(window, cx),
                    StoryContainer::panel::<DialogStory>(window, cx),
                    StoryContainer::panel::<EditorStory>(window, cx),
                    StoryContainer::panel::<FloatingToolbarStory>(window, cx),
                    StoryContainer::panel::<FormStory>(window, cx),
                    StoryContainer::panel::<GroupBoxStory>(window, cx),
                    StoryContainer::panel::<HoverCardStory>(window, cx),
                    StoryContainer::panel::<IconStory>(window, cx),
                    StoryContainer::panel::<IconButtonStory>(window, cx),
                    StoryContainer::panel::<ImageStory>(window, cx),
                    StoryContainer::panel::<InputStory>(window, cx),
                    StoryContainer::panel::<KbdStory>(window, cx),
                    StoryContainer::panel::<LabelStory>(window, cx),
                    StoryContainer::panel::<ListStory>(window, cx),
                    StoryContainer::panel::<MenuStory>(window, cx),
                    StoryContainer::panel::<NativeMenuStory>(window, cx),
                    StoryContainer::panel::<NavigationRailStory>(window, cx),
                    StoryContainer::panel::<NotificationStory>(window, cx),
                    StoryContainer::panel::<NumberInputStory>(window, cx),
                    StoryContainer::panel::<OtpInputStory>(window, cx),
                    StoryContainer::panel::<PaginationStory>(window, cx),
                    StoryContainer::panel::<PopoverStory>(window, cx),
                    StoryContainer::panel::<ProgressStory>(window, cx),
                    StoryContainer::panel::<RadioStory>(window, cx),
                    StoryContainer::panel::<RatingStory>(window, cx),
                    StoryContainer::panel::<ResizableStory>(window, cx),
                    StoryContainer::panel::<ScrollbarStory>(window, cx),
                    StoryContainer::panel::<SelectStory>(window, cx),
                    StoryContainer::panel::<SeparatorStory>(window, cx),
                    StoryContainer::panel::<SettingsStory>(window, cx),
                    StoryContainer::panel::<SheetStory>(window, cx),
                    StoryContainer::panel::<SidebarStory>(window, cx),
                    StoryContainer::panel::<SkeletonStory>(window, cx),
                    StoryContainer::panel::<SliderStory>(window, cx),
                    StoryContainer::panel::<SplitButtonStory>(window, cx),
                    StoryContainer::panel::<StatusBarStory>(window, cx),
                    StoryContainer::panel::<StepperStory>(window, cx),
                    StoryContainer::panel::<SwitchStory>(window, cx),
                    StoryContainer::panel::<ToggleButtonStory>(window, cx),
                    StoryContainer::panel::<DataTableStory>(window, cx),
                    StoryContainer::panel::<TableStory>(window, cx),
                    StoryContainer::panel::<TabsStory>(window, cx),
                    StoryContainer::panel::<TagStory>(window, cx),
                    StoryContainer::panel::<TextareaStory>(window, cx),
                    StoryContainer::panel::<ThemeColorsStory>(window, cx),
                    StoryContainer::panel::<TooltipStory>(window, cx),
                    StoryContainer::panel::<TreeStory>(window, cx),
                    StoryContainer::panel::<VirtualListStory>(window, cx),
                ],
            ),
        ];

        let mut this = Self {
            search_input,
            stories,
            active_group_index: Some(0),
            active_index: Some(0),
            collapsed: false,
            _subscriptions,
        };

        if let Some(init_story) = init_story {
            this.set_active_story(init_story, window, cx);
        }

        this
    }

    fn set_active_story(&mut self, name: &str, window: &mut Window, cx: &mut App) {
        let name = name.to_string();
        self.search_input.update(cx, |this, cx| {
            this.set_value(&name, window, cx);
        })
    }

    pub fn view(init_story: Option<&str>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(init_story, window, cx))
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search_input.read(cx).value().trim().to_lowercase();

        let stories: Vec<_> = self
            .stories
            .iter()
            .filter_map(|(name, items)| {
                let filtered_items: Vec<_> = items
                    .iter()
                    .filter(|story| story.read(cx).name.to_lowercase().contains(&query))
                    .cloned()
                    .collect();

                if !filtered_items.is_empty() {
                    Some((name, filtered_items))
                } else {
                    None
                }
            })
            .collect();

        let active_group = self.active_group_index.and_then(|index| stories.get(index));
        let active_story = self
            .active_index
            .and(active_group)
            .and_then(|group| group.1.get(self.active_index.unwrap()));
        let (story_name, description) =
            if let Some(story) = active_story.as_ref().map(|story| story.read(cx)) {
                (story.name.clone(), story.description.clone())
            } else {
                ("".into(), "".into())
            };

        let current_story = story_name.clone();
        let total_components: usize = self.stories.iter().map(|(_, items)| items.len()).sum();

        let body = h_resizable("gallery-container")
            .child(
                resizable_panel()
                    .size(px(255.))
                    .size_range(px(200.)..px(320.))
                    .child(
                        Sidebar::new("gallery-sidebar")
                            .w(relative(1.))
                            .border(false)
                            .collapsed(self.collapsed)
                            .header(
                                v_flex().w_full().pt_2().child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .gap_x_2()
                                        .px_2()
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .size_8()
                                                .rounded_full()
                                                .bg(if cx.theme().mode.is_dark() {
                                                    cx.theme().surface_container_high
                                                } else {
                                                    cx.theme().primary_container
                                                })
                                                .child(
                                                    Icon::new(IconName::Palette)
                                                        .size_5()
                                                        .text_color(cx.theme().primary),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .bg(cx.theme().primary_container)
                                                .rounded_full()
                                                .px_1()
                                                .when(cx.theme().radius.is_zero(), |this| {
                                                    this.rounded(px(0.))
                                                })
                                                .flex_1()
                                                .child(
                                                    Input::new(&self.search_input)
                                                        .appearance(false)
                                                        .cleanable(true),
                                                ),
                                        ),
                                ),
                            )
                            .children(stories.clone().into_iter().enumerate().map(
                                |(group_ix, (group_name, sub_stories))| {
                                    SidebarGroup::new(*group_name).child(
                                        SidebarMenu::new().children(
                                            sub_stories.iter().enumerate().map(|(ix, story)| {
                                                SidebarMenuItem::new(story.read(cx).name.clone())
                                                    .active(
                                                        self.active_group_index == Some(group_ix)
                                                            && self.active_index == Some(ix),
                                                    )
                                                    .on_click(cx.listener(
                                                        move |this, _: &ClickEvent, _, cx| {
                                                            this.active_group_index =
                                                                Some(group_ix);
                                                            this.active_index = Some(ix);
                                                            cx.notify();
                                                        },
                                                    ))
                                            }),
                                        ),
                                    )
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .p_2()
                    .child(
                        v_flex()
                            .id("content-card")
                            .size_full()
                            .overflow_x_hidden()
                            .bg(cx.theme().surface)
                            .rounded(cx.theme().radius_lg * 2.5)
                            .child(
                                h_flex()
                                    .id("header")
                                    .p_4()
                                    .justify_between()
                                    .items_start()
                                    .child(
                                        v_flex()
                                            .gap_1()
                                            .child(div().text_xl().child(story_name))
                                            .child(
                                                div()
                                                    .text_color(cx.theme().on_surface_variant)
                                                    .child(description),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("story")
                                    .flex_1()
                                    .overflow_y_scroll()
                                    .when_some(active_story, |this, active_story| {
                                        this.child(active_story.clone())
                                    }),
                            ),
                    )
                    .into_any_element(),
            );

        v_flex()
            .size_full()
            .child(div().flex_1().min_h_0().child(body))
            .child(
                StatusBar::new()
                    .child(Icon::new(IconName::Dashboard).xsmall())
                    .child(format!("{total_components} components"))
                    .child(Separator::vertical())
                    .when(!current_story.is_empty(), |this| {
                        this.child(current_story.clone())
                    })
                    .right(format!("Material source #{:08x}", cx.theme().source_argb))
                    .right(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .right(
                        Button::new("assistant")
                            .text()
                            .xsmall()
                            .icon(IconName::Globe)
                            .tooltip("GPUI Component GitHub repository")
                            .on_click(|_, _, cx| {
                                cx.open_url("https://github.com/sayeed205/shilpo")
                            }),
                    ),
            )
    }
}
