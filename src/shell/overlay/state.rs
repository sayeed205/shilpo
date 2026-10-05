use amane::{Keyboard, Notification, Notifications, Service};

use super::panel::Panel;
use super::utility::{self, Page};
use crate::ui::motion::{self, Glide};

// how long the power menu's hover fill and the launcher's list take to move
const FILL_DURATION: u64 = 340;
pub const LIST_DURATION: u64 = 240;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PanelKind {
    PowerMenu,
    ControlCenter,
    Launcher,
    Utility,
}

// which panels are out, shared by the bar that opens them and the overlay that draws them
pub(crate) struct Overlay {
    pub(super) power_menu: Panel,

    // how far each power menu button's hover color has filled it, 0 to 1
    pub(super) action_fills: Vec<Glide>,

    // leaving a panel for the bar keeps it open
    pub(super) bar_hovered: bool,

    pub(super) control_center: Panel,

    pub(super) launcher: Panel,

    pub(super) query: String,

    // the chosen result, and the first result the list shows
    pub(super) selected: usize,
    pub(super) first: usize,

    pub(super) hovered_row: Option<usize>,

    // which result the highlight is on, and which one is at the top of the list, both sliding
    pub(super) highlight: Glide,
    pub(super) scroll: Glide,

    pub(super) sessions: Vec<String>,

    pub(super) utility: Panel,

    pub(super) page: Page,

    // the control under the pointer in the utility center, like "tab:wifi"
    pub(super) hovered: Option<String>,

    // the network whose password is being typed
    pub(super) password_for: Option<String>,
    pub(super) show_password: bool,

    // the month the calendar shows, counted from this month
    pub(super) calendar_month: i64,

    // the network last asked to join, shown as connecting until it is up
    pub(super) joining: Option<String>,

    // the popup under the pointer, whose countdown waits
    // the media player the control center shows, by bus name; none shows the active one
    pub(super) player: Option<String>,

    // slides the media card in from the side it was switched toward, 0 once settled
    pub(super) player_switch: Glide,

    pub(super) hovered_popup: Option<u32>,

    // popups closed by hand, which leave before their time
    pub(super) closed_popups: Vec<u32>,
}

impl Service for Overlay {
    fn new() -> Self {
        let mut action_fills = Vec::new();

        for _ in 0..super::power_menu::ACTION_COUNT {
            action_fills.push(motion::spatial(0.0, FILL_DURATION));
        }

        Self {
            power_menu: Panel::new(),
            action_fills,
            bar_hovered: false,
            control_center: Panel::new(),
            launcher: Panel::new(),
            query: String::new(),
            selected: 0,
            first: 0,
            hovered_row: None,
            highlight: motion::spatial(0.0, LIST_DURATION),
            scroll: motion::spatial(0.0, LIST_DURATION),
            sessions: Vec::new(),
            utility: Panel::new(),
            page: Page::Notifications,
            hovered: None,
            password_for: None,
            show_password: false,
            joining: None,
            calendar_month: 0,
            player: None,
            player_switch: motion::spatial(0.0, motion::FAST_SPATIAL),
            hovered_popup: None,
            closed_popups: Vec::new(),
        }
    }

    // only changes through input
    fn listen() {}
}

impl Overlay {
    pub(crate) fn toggle_power_menu() {
        Self::write().toggle_power_menu_state();
    }

    fn toggle_power_menu_state(&mut self) {
        let mut close_utility = utility::close;

        self.hide_other_panels(PanelKind::PowerMenu, &mut close_utility);

        self.power_menu.toggle();
    }

    pub(super) fn hide_power_menu() {
        Self::write().power_menu.hide();
    }

    pub(crate) fn toggle_utility() {
        Self::write().toggle_utility_state(utility::close, utility::opened);
    }

    fn toggle_utility_state(
        &mut self,
        mut close_utility: impl FnMut(&mut Overlay),
        open_utility: impl FnOnce(&mut Overlay),
    ) {
        self.hide_other_panels(PanelKind::Utility, &mut close_utility);

        if self.utility.shown {
            close_utility(self);

            return;
        }

        self.utility.show();

        open_utility(self);
    }

    pub(crate) fn toggle_control_center() {
        Self::write().toggle_control_center_state();
    }

    fn toggle_control_center_state(&mut self) {
        let mut close_utility = utility::close;

        self.hide_other_panels(PanelKind::ControlCenter, &mut close_utility);

        self.control_center.toggle();
    }

    pub(crate) fn hover_control_center(inside: bool) {
        Self::write().hover_panel(PanelKind::ControlCenter, inside);
    }

    pub(crate) fn hover_power_menu(inside: bool) {
        Self::write().hover_panel(PanelKind::PowerMenu, inside);
    }

    pub(crate) fn hover_utility(inside: bool) {
        Self::write().hover_panel(PanelKind::Utility, inside);
    }

    fn hover_panel(&mut self, panel: PanelKind, inside: bool) {
        let panel = match panel {
            PanelKind::PowerMenu => &mut self.power_menu,
            PanelKind::ControlCenter => &mut self.control_center,
            PanelKind::Launcher => &mut self.launcher,
            PanelKind::Utility => &mut self.utility,
        };

        panel.hovered = inside;

        if inside {
            panel.was_hovered = true;
        }

        self.dismiss_if_left();
    }

    pub(super) fn hover_popup(id: u32, inside: bool) {
        let mut overlay = Self::write();

        if inside {
            overlay.hovered_popup = Some(id);
        } else if overlay.hovered_popup == Some(id) {
            overlay.hovered_popup = None;
        }
    }

    // only the popup goes, the notification stays in the list
    pub(super) fn close_popup(id: u32) {
        let listed: Vec<u32> = Notifications::read().list().iter().map(Notification::id).collect();

        let mut overlay = Self::write();

        // ids of notifications already gone are dropped, so the list stays short
        overlay.closed_popups.retain(|closed| listed.contains(closed));

        overlay.closed_popups.push(id);
    }

    pub(crate) fn hover_bar(inside: bool) {
        Self::write().set_bar_hovered(inside);
    }

    fn set_bar_hovered(&mut self, inside: bool) {
        self.bar_hovered = inside;

        self.dismiss_if_left();
    }

    // A panel opening owns the exclusion invariant; utility teardown runs last as before.
    fn hide_other_panels(
        &mut self,
        keep: PanelKind,
        close_utility: &mut impl FnMut(&mut Overlay),
    ) {
        if keep != PanelKind::Launcher {
            self.launcher.hide();
        }

        if keep != PanelKind::PowerMenu {
            self.power_menu.hide();
        }

        if keep != PanelKind::ControlCenter {
            self.control_center.hide();
        }

        if keep != PanelKind::Utility {
            close_utility(self);
        }
    }

    // Shared by launcher::show after panel exclusion, before its query is synchronized.
    pub(super) fn show_launcher(&mut self) {
        let mut close_utility = utility::close;

        self.hide_other_panels(PanelKind::Launcher, &mut close_utility);

        self.launcher.show();
    }

    pub(super) fn any_panel_open(&self) -> bool {
        self.power_menu.shown
            || self.control_center.shown
            || self.launcher.shown
            || self.utility.shown
    }

    pub(super) fn close_all(&mut self) {
        self.power_menu.hide();
        self.control_center.hide();
        self.launcher.hide();
        utility::close(self);
    }

    pub(super) fn keyboard(&self) -> Keyboard {
        if self.launcher.shown || (self.utility.shown && self.password_for.is_some()) {
            Keyboard::Exclusive
        } else if self.utility.shown {
            Keyboard::OnDemand
        } else {
            Keyboard::None
        }
    }

    /*
     * a panel closes once the pointer has been on it and then left it and
     * the bar; not while a password is being typed, it would be lost
     */
    fn dismiss_if_left(&mut self) {
        let bar_hovered = self.bar_hovered;

        let typing = self.password_for.is_some();

        let mut panels = vec![&mut self.power_menu, &mut self.control_center];

        if !typing {
            panels.push(&mut self.utility);
        }

        for panel in panels {
            let left = panel.was_hovered && !panel.hovered && !bar_hovered;

            if panel.shown && left {
                panel.hide();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use amane::Service;

    use super::{Overlay, Page, PanelKind};

    #[test]
    fn opening_each_panel_excludes_the_other_panels() {
        for keep in [
            PanelKind::PowerMenu,
            PanelKind::ControlCenter,
            PanelKind::Utility,
            PanelKind::Launcher,
        ] {
            let mut overlay = Overlay::new();
            overlay.page = Page::Notifications;

            if keep != PanelKind::PowerMenu {
                overlay.power_menu.show();
            }
            if keep != PanelKind::ControlCenter {
                overlay.control_center.show();
            }
            if keep != PanelKind::Utility {
                overlay.utility.show();
            }
            if keep != PanelKind::Launcher {
                overlay.launcher.show();
            }

            match keep {
                PanelKind::PowerMenu => overlay.toggle_power_menu_state(),
                PanelKind::ControlCenter => overlay.toggle_control_center_state(),
                PanelKind::Utility => overlay.toggle_utility_state(|_| {}, |_| {}),
                PanelKind::Launcher => overlay.show_launcher(),
            }

            assert_eq!(
                [
                    overlay.power_menu.shown,
                    overlay.control_center.shown,
                    overlay.utility.shown,
                    overlay.launcher.shown,
                ],
                [
                    keep == PanelKind::PowerMenu,
                    keep == PanelKind::ControlCenter,
                    keep == PanelKind::Utility,
                    keep == PanelKind::Launcher,
                ],
            );
        }
    }

    #[test]
    fn pointer_dismissal_requires_entry_then_leaving_panel_and_bar() {
        let mut overlay = Overlay::new();
        overlay.power_menu.show();

        overlay.hover_panel(PanelKind::PowerMenu, false);
        assert!(overlay.power_menu.shown, "must have been hovered first");

        overlay.hover_panel(PanelKind::PowerMenu, true);
        overlay.set_bar_hovered(true);
        overlay.hover_panel(PanelKind::PowerMenu, false);
        assert!(overlay.power_menu.shown, "the bar keeps the panel open");

        overlay.set_bar_hovered(false);
        assert!(!overlay.power_menu.shown);
    }

    #[test]
    fn password_protects_utility_but_pointer_dismissal_does_not_teardown_scan() {
        let mut overlay = Overlay::new();
        overlay.utility.show();
        overlay.page = Page::Bluetooth;
        overlay.password_for = Some(String::from("network"));

        overlay.hover_panel(PanelKind::Utility, true);
        overlay.hover_panel(PanelKind::Utility, false);
        assert!(overlay.utility.shown, "typing protects the utility panel");

        overlay.password_for = None;
        overlay.dismiss_if_left();
        assert!(!overlay.utility.shown);
        assert!(overlay.page == Page::Bluetooth);
        // Pointer dismissal hides directly; only the explicit close path stops scanning.
    }

    #[test]
    fn dismiss_predicate_close_all_and_keyboard_policy_are_owned_by_overlay() {
        let mut overlay = Overlay::new();
        assert!(!overlay.any_panel_open());
        assert!(matches!(overlay.keyboard(), amane::Keyboard::None));

        overlay.utility.show();
        assert!(overlay.any_panel_open());
        assert!(matches!(overlay.keyboard(), amane::Keyboard::OnDemand));

        overlay.password_for = Some(String::from("network"));
        assert!(matches!(overlay.keyboard(), amane::Keyboard::Exclusive));

        overlay.password_for = None;
        overlay.launcher.show();
        assert!(matches!(overlay.keyboard(), amane::Keyboard::Exclusive));

        overlay.power_menu.show();
        overlay.control_center.show();
        overlay.close_all();
        assert!(!overlay.any_panel_open());
        assert!(matches!(overlay.keyboard(), amane::Keyboard::None));
    }
}
