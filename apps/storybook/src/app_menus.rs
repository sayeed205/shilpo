use gpui::{App, Entity, Menu, MenuItem, SharedString};
use shilpo_ui::{ActiveTheme as _, GlobalState, Theme, ThemeMode, menu::AppMenuBar};
use std::cell::Cell;

use crate::{
    About, AppState, Open, Quit, SelectLocale, SelectWindowControls, ToggleSearch,
    themes::SwitchThemeMode,
};
use shilpo_ui::WindowControlsMode;

pub fn init(title: impl Into<SharedString>, cx: &mut App) -> Entity<AppMenuBar> {
    let app_menu_bar = AppMenuBar::new(cx);
    let title: SharedString = title.into();
    update_app_menu(title.clone(), app_menu_bar.clone(), cx);

    cx.on_action({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |s: &SelectLocale, cx: &mut App| {
            rust_i18n::set_locale(&s.0.as_str());
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    });

    cx.on_action({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |select: &SelectWindowControls, cx: &mut App| {
            AppState::global_mut(cx).window_controls = select.0;
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    });

    // Observe theme changes to update the menu to refresh the checked state
    let selected_mode = Cell::new(cx.theme().selected_mode());
    cx.observe_global::<Theme>({
        let title = title.clone();
        let app_menu_bar = app_menu_bar.clone();
        move |cx| {
            let mode = cx.theme().selected_mode();
            if selected_mode.get() == mode {
                return;
            }
            selected_mode.set(mode);
            update_app_menu(title.clone(), app_menu_bar.clone(), cx);
        }
    })
    .detach();

    app_menu_bar
}

fn update_app_menu(title: impl Into<SharedString>, app_menu_bar: Entity<AppMenuBar>, cx: &mut App) {
    let title: SharedString = title.into();

    cx.set_menus(build_menus(title.clone(), cx));
    let menus = build_menus(title, cx)
        .into_iter()
        .map(|menu| menu.owned())
        .collect();
    GlobalState::global_mut(cx).set_app_menus(menus);

    app_menu_bar.update(cx, |menu_bar, cx| {
        menu_bar.reload(cx);
    })
}

fn build_menus(title: impl Into<SharedString>, cx: &App) -> Vec<Menu> {
    vec![
        Menu {
            name: title.into(),
            items: vec![
                MenuItem::action("About", About),
                MenuItem::Separator,
                MenuItem::action("Open...", Open),
                MenuItem::Separator,
                MenuItem::Submenu(Menu {
                    name: "Appearance".into(),
                    items: vec![
                        MenuItem::action("Light", SwitchThemeMode(ThemeMode::Light))
                            .checked(cx.theme().selected_mode() == ThemeMode::Light),
                        MenuItem::action("Dark", SwitchThemeMode(ThemeMode::Dark))
                            .checked(cx.theme().selected_mode() == ThemeMode::Dark),
                        MenuItem::action("System", SwitchThemeMode(ThemeMode::System))
                            .checked(cx.theme().selected_mode() == ThemeMode::System),
                    ],
                    disabled: false,
                }),
                language_menu(cx),
                MenuItem::Separator,
                MenuItem::action("Quit", Quit),
            ],
            disabled: false,
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Undo", shilpo_ui::input::Undo),
                MenuItem::action("Redo", shilpo_ui::input::Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", shilpo_ui::input::Cut),
                MenuItem::action("Copy", shilpo_ui::input::Copy),
                MenuItem::action("Paste", shilpo_ui::input::Paste),
                MenuItem::separator(),
                MenuItem::action("Delete", shilpo_ui::input::Delete),
                MenuItem::action(
                    "Delete Previous Word",
                    shilpo_ui::input::DeleteToPreviousWordStart,
                ),
                MenuItem::action("Delete Next Word", shilpo_ui::input::DeleteToNextWordEnd),
                MenuItem::separator(),
                MenuItem::action("Find", shilpo_ui::input::Search),
                MenuItem::separator(),
                MenuItem::action("Select All", shilpo_ui::input::SelectAll),
            ],
            disabled: false,
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Toggle Search", ToggleSearch),
                MenuItem::Separator,
                MenuItem::Submenu(Menu {
                    name: "Window Controls".into(),
                    items: vec![
                        MenuItem::action(
                            "Automatic",
                            SelectWindowControls(WindowControlsMode::Auto),
                        )
                        .checked(AppState::global(cx).window_controls == WindowControlsMode::Auto),
                        MenuItem::action("Show", SelectWindowControls(WindowControlsMode::Show))
                            .checked(
                                AppState::global(cx).window_controls == WindowControlsMode::Show,
                            ),
                        MenuItem::action("Hide", SelectWindowControls(WindowControlsMode::Hide))
                            .checked(
                                AppState::global(cx).window_controls == WindowControlsMode::Hide,
                            ),
                    ],
                    disabled: false,
                }),
            ],
            disabled: false,
        },
        Menu {
            name: "Help".into(),
            items: vec![
                MenuItem::action("Documentation", Open).disabled(true),
                MenuItem::separator(),
                MenuItem::action("Open Website", Open),
            ],
            disabled: false,
        },
    ]
}

fn language_menu(_: &App) -> MenuItem {
    let locale = rust_i18n::locale().to_string();
    MenuItem::Submenu(Menu {
        name: "Language".into(),
        items: vec![
            MenuItem::action("English", SelectLocale("en".into())).checked(locale == "en"),
            MenuItem::action("简体中文", SelectLocale("zh-CN".into())).checked(locale == "zh-CN"),
            MenuItem::action("Français", SelectLocale("fr".into())).checked(locale == "fr"),
        ],
        disabled: false,
    })
}
