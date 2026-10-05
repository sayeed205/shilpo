mod bluetooth;
mod brightness;
mod calendar;
mod header;
pub mod notifications;
mod switch;
mod tabs;
mod wifi;

use amane::{Bluetooth, Column, Key, Network, Padding, Rectangle, Service, Stack, Widget, children};

use super::{Overlay, PanelView, Region};
use crate::ui::liquid::{self, Blob};
use crate::ui::motion::{self, DEFAULT_SPATIAL};
use crate::ui::theme::Theme;

const WIDTH: f32 = 360.0;
const MAX_HEIGHT: f32 = 800.0;
const RADIUS: f32 = 30.0;

// the panel reaches this far past the screen's right edge, so it stays melted into it
const EDGE_OVERLAP: f32 = 40.0;

// a little above the window's top, so it melts into the bar's edge too
const TOP: f32 = -10.0;

// how far past its own width it slides to let go of the edge
const HIDDEN_MARGIN: f32 = 32.0;

const PADDING: Padding = Padding {
    top: 28.0,
    right: EDGE_OVERLAP + 16.0,
    bottom: 16.0,
    left: 16.0,
};

const INNER_WIDTH: f32 = WIDTH - PADDING.left - PADDING.right;

const GAP: f32 = 12.0;

// the pages side by side, in the order of the tabs
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Page {
    Notifications,
    Wifi,
    Bluetooth,
}

impl Page {
    fn index(self) -> f32 {
        match self {
            Page::Notifications => 0.0,
            Page::Wifi => 1.0,
            Page::Bluetooth => 2.0,
        }
    }
}

// none while fully hidden
pub fn view(overlay: &Overlay, theme: &Theme, screen: Region) -> Option<PanelView> {
    let progress = overlay.utility.progress.value();

    if progress <= 0.001 {
        return None;
    }

    let height = MAX_HEIGHT.min(screen.height - 96.0);

    let hidden = WIDTH + HIDDEN_MARGIN;

    let x = screen.width - WIDTH + EDGE_OVERLAP + hidden * (1.0 - progress);

    let blob = Blob::new(x, TOP, WIDTH, height)
        .radius(RADIUS)
        .child(content(overlay, theme, height));

    let input = Region {
        x,
        y: TOP,
        width: WIDTH,
        height,
    };

    // fully out it starts at its resting edge, and its melted corners spread a little past that
    let reach_width = WIDTH - EDGE_OVERLAP + liquid::CONNECTION;

    let reach = Region {
        x: screen.width - reach_width,
        y: 0.0,
        width: reach_width,
        height: TOP + height + liquid::CONNECTION,
    };

    Some(PanelView { blob, input, reach })
}

fn content(overlay: &Overlay, theme: &Theme, height: f32) -> Rectangle {
    let fixed = tabs::HEIGHT + brightness::HEIGHT + calendar::HEIGHT + GAP * 3.0;

    let pages_height = height - PADDING.top - PADDING.bottom - fixed;

    let column = Column::new(children![
        tabs::view(overlay, theme, INNER_WIDTH),
        brightness::view(overlay, theme, INNER_WIDTH),
        pages(overlay, theme, pages_height),
        calendar::view(overlay, theme, INNER_WIDTH),
    ])
    .gap(GAP);

    Rectangle::new()
        .width(WIDTH)
        .height(height)
        .padding(PADDING)
        .on_hover(Overlay::hover_utility)
        .child(column)
}

// every page is a full width apart, and they all slide together to the chosen one
fn pages(overlay: &Overlay, theme: &Theme, height: f32) -> Rectangle {
    let shown = motion::follow("utility-page", overlay.page.index(), DEFAULT_SPATIAL);

    let mut layers: Vec<Box<dyn Widget>> = Vec::new();

    for page in [Page::Notifications, Page::Wifi, Page::Bluetooth] {
        let offset = page.index() - shown;

        // only pages at least partly in view are built
        if offset.abs() >= 1.0 {
            continue;
        }

        let content = match page {
            Page::Notifications => notifications::view(overlay, theme, INNER_WIDTH, height),
            Page::Wifi => wifi::view(overlay, theme, INNER_WIDTH, height),
            Page::Bluetooth => bluetooth::view(overlay, theme, INNER_WIDTH, height),
        };

        let slot = Rectangle::new()
            .width(INNER_WIDTH)
            .height(height)
            .translate(offset * INNER_WIDTH, 0.0)
            .child(content);

        layers.push(Box::new(slot));
    }

    let viewport = Rectangle::new()
        .width(INNER_WIDTH)
        .height(height)
        .child(Stack::new(layers));

    // a clip is a gpu layer, so only while a page slides past the edge
    if shown != overlay.page.index() {
        return viewport.clip();
    }

    viewport
}

// what opening the panel or turning to a page starts, like a fresh wifi scan
pub(super) fn opened(overlay: &mut Overlay) {
    opened_with(overlay, close_password, |page| match page {
        Page::Wifi => Network::scan(),
        Page::Bluetooth => Bluetooth::start_scan(),
        Page::Notifications => {}
    });
}

// every way the panel closes goes through here, so a bluetooth scan never outlives it
pub(super) fn close(overlay: &mut Overlay) {
    close_with(overlay, |_| Bluetooth::stop_scan());
}

fn close_with(overlay: &mut Overlay, stop_scan: impl FnOnce(&Overlay)) {
    if overlay.utility.shown && overlay.page == Page::Bluetooth {
        stop_scan(overlay);
    }

    overlay.utility.hide();
}

fn opened_with(
    overlay: &mut Overlay,
    clear_password: impl FnOnce(&mut Overlay),
    mut scan: impl FnMut(Page),
) {
    clear_password(overlay);

    match overlay.page {
        Page::Wifi => scan(Page::Wifi),
        Page::Bluetooth => scan(Page::Bluetooth),
        Page::Notifications => {}
    }
}

// "toggle", "show" or "hide", then optionally the page to show
pub fn ipc(arguments: &[String]) -> String {
    let command = arguments.first().map(String::as_str).unwrap_or("toggle");

    let page = match arguments.get(1).map(String::as_str) {
        Some("notifications") => Some(Page::Notifications),
        Some("wifi") => Some(Page::Wifi),
        Some("bluetooth") => Some(Page::Bluetooth),
        _ => None,
    };

    if let Some(page) = page {
        assign_ipc_page(&mut Overlay::write(), page);
    }

    let shown = Overlay::read().utility.shown;

    let wanted = match command {
        "show" => true,
        "hide" => false,
        _ => !shown,
    };

    if wanted != shown {
        Overlay::toggle_utility();
    }

    String::from("ok")
}

pub(super) fn select(page: Page) {
    select_with(&mut Overlay::write(), page, Bluetooth::stop_scan, opened);
}

fn assign_ipc_page(overlay: &mut Overlay, page: Page) {
    overlay.page = page;
}

fn select_with(
    overlay: &mut Overlay,
    page: Page,
    stop_bluetooth_scan: impl FnOnce(),
    open_page: impl FnOnce(&mut Overlay),
) {
    if overlay.page == page {
        return;
    }

    if overlay.page == Page::Bluetooth {
        stop_bluetooth_scan();
    }

    overlay.page = page;

    open_page(overlay);
}

// escape closes an open password field first, then the panel
pub fn key_pressed(key: Key) {
    key_pressed_with(
        &mut Overlay::write(),
        key,
        close_password,
        close,
    );
}

fn key_pressed_with(
    overlay: &mut Overlay,
    key: Key,
    clear_password: impl FnOnce(&mut Overlay),
    close_panel: impl FnOnce(&mut Overlay),
) {
    if !overlay.utility.shown || key != Key::Escape {
        return;
    }

    if overlay.password_for.is_some() {
        clear_password(overlay);

        return;
    }

    close_panel(overlay);
}

pub(super) fn close_password(overlay: &mut Overlay) {
    clear_password_state(overlay);

    wifi::clear_password();
}

fn clear_password_state(overlay: &mut Overlay) {
    overlay.password_for = None;
    overlay.show_password = false;
}

// remembers which control the pointer is on, for hover colors
pub fn hover(name: String, inside: bool) {
    let mut overlay = Overlay::write();

    if inside {
        overlay.hovered = Some(name);
    } else if overlay.hovered.as_ref() == Some(&name) {
        overlay.hovered = None;
    }
}

pub fn hovered(overlay: &Overlay, name: &str) -> bool {
    overlay.hovered.as_deref() == Some(name)
}

// where a hover color fades to: 1 while the pointer is on the control
pub fn fade_target(overlay: &Overlay, name: &str) -> f32 {
    if hovered(overlay, name) { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use amane::Service;

    use super::{
        Page, assign_ipc_page, clear_password_state, close_with, key_pressed_with, opened_with,
        select_with,
    };
    use crate::shell::overlay::state::Overlay;

    #[test]
    fn explicit_close_stops_only_an_open_bluetooth_scan_before_hiding() {
        let mut overlay = Overlay::new();
        overlay.utility.show();
        overlay.page = Page::Bluetooth;

        let events = RefCell::new(Vec::new());
        close_with(&mut overlay, |state| {
            assert!(state.utility.shown, "stop scan precedes hiding");
            events.borrow_mut().push("stop bluetooth scan");
        });

        assert_eq!(*events.borrow(), ["stop bluetooth scan"]);
        assert!(!overlay.utility.shown);

        let mut overlay = Overlay::new();
        overlay.page = Page::Bluetooth;
        close_with(&mut overlay, |_| panic!("a hidden utility has no scan to stop"));

        let mut overlay = Overlay::new();
        overlay.utility.show();
        overlay.page = Page::Wifi;
        close_with(&mut overlay, |_| panic!("wifi scans are not stopped here"));
    }

    #[test]
    fn ipc_page_assignment_does_not_run_tab_selection_effects() {
        let mut overlay = Overlay::new();
        overlay.utility.show();
        overlay.page = Page::Bluetooth;
        overlay.password_for = Some(String::from("network"));
        overlay.show_password = true;

        assign_ipc_page(&mut overlay, Page::Wifi);

        assert!(overlay.page == Page::Wifi);
        assert!(overlay.utility.shown);
        assert_eq!(overlay.password_for.as_deref(), Some("network"));
        assert!(overlay.show_password);
    }

    #[test]
    fn tab_selection_stops_old_bluetooth_then_clears_password_and_scans_new_page() {
        let mut overlay = Overlay::new();
        overlay.utility.show();
        overlay.page = Page::Bluetooth;
        overlay.password_for = Some(String::from("network"));
        overlay.show_password = true;

        let events = RefCell::new(Vec::new());
        select_with(
            &mut overlay,
            Page::Wifi,
            || events.borrow_mut().push("stop bluetooth scan"),
            |overlay| {
                opened_with(
                    overlay,
                    |overlay| {
                        clear_password_state(overlay);
                        events.borrow_mut().push("clear password");
                    },
                    |page| {
                        events.borrow_mut().push(match page {
                            Page::Wifi => "scan wifi",
                            Page::Bluetooth => "scan bluetooth",
                            Page::Notifications => "no scan",
                        });
                    },
                );
            },
        );

        assert_eq!(
            *events.borrow(),
            ["stop bluetooth scan", "clear password", "scan wifi"],
        );
        assert!(overlay.page == Page::Wifi);
        assert_eq!(overlay.password_for, None);
        assert!(!overlay.show_password);

        select_with(
            &mut overlay,
            Page::Wifi,
            || panic!("same-page selection has no scan teardown"),
            |_| panic!("same-page selection does not reopen the page"),
        );
    }

    #[test]
    fn escape_clears_password_before_a_later_escape_closes_the_panel() {
        let mut overlay = Overlay::new();
        overlay.utility.show();
        overlay.page = Page::Bluetooth;
        overlay.password_for = Some(String::from("network"));
        overlay.show_password = true;

        let events = RefCell::new(Vec::new());
        key_pressed_with(
            &mut overlay,
            amane::Key::Escape,
            |overlay| {
                clear_password_state(overlay);
                events.borrow_mut().push("clear password");
            },
            |_| panic!("password Escape must not close the panel"),
        );

        assert!(overlay.utility.shown);
        assert_eq!(overlay.password_for, None);
        assert!(!overlay.show_password);

        key_pressed_with(
            &mut overlay,
            amane::Key::Escape,
            |_| panic!("there is no password left to clear"),
            |overlay| {
                close_with(overlay, |state| {
                    assert!(state.utility.shown, "scan stops before the panel is hidden");
                    events.borrow_mut().push("stop bluetooth scan");
                });
            },
        );

        assert_eq!(*events.borrow(), ["clear password", "stop bluetooth scan"]);
        assert!(!overlay.utility.shown);
    }
}
