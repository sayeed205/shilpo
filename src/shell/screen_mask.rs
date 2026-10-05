use amane::{Full, Layer, LayerWindow, Mask, Monitor, Parent, Rectangle, Service, Zone};

use crate::shell::lock_screen::Curtain;
use crate::ui::theme;
use crate::shell::wallpaper::SCREEN_RADIUS;

/*
 * the bar's color with a rounded hole the size of the area below the bar,
 * above windows, so the screen's corners look rounded like the bar flows
 * into them; it takes no clicks
 */
pub fn view(_monitor: &Monitor) -> LayerWindow {
    let theme = theme::current();

    let hole = Rectangle::new()
        .width(Parent)
        .height(Parent)
        .radius(SCREEN_RADIUS)
        .fill(Mask);

    LayerWindow::new()
        .width(Full)
        .height(Full)
        .layer(Layer::Top)
        .space(Zone::Respect)
        .namespace("screen-mask")
        .click_through()
        .child(
            Rectangle::new()
                .width(Parent)
                .height(Parent)
                .fill(theme.background)
                .opacity(Curtain::read().items.value())
                .child(hole),
        )
}
