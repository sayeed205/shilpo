use amane::{Color, Full, Layer, LayerWindow, Monitor, Parent, Rectangle, Service, Zone};

use super::Overlay;
use crate::config::Settings;

/*
 * a clear window over the whole screen, under the panels, while one is open
 * and click outside to dismiss is on; a click on it closes every panel.
 * it draws nothing, so unlike the liquid it costs nothing to cover the screen
 */
pub fn view(_monitor: &Monitor) -> LayerWindow {
    let overlay = Overlay::read();

    let open = overlay.any_panel_open();

    drop(overlay);

    let catching = open && Settings::read().flag("click_outside_dismiss");

    LayerWindow::new()
        .width(Full)
        .height(Full)
        .layer(Layer::Top)
        .space(Zone::Respect)
        .namespace("dismiss")
        .visible(catching)
        .child(
            Rectangle::new()
                .width(Parent)
                .height(Parent)
                .fill(Color::TRANSPARENT)
                .on_click(|_| close_all()),
        )
}

fn close_all() {
    Overlay::write().close_all();
}
