use gpui::{
    Bounds, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};
use shilpo_assets::Assets;
use shilpo_shell::bar::BarView;

fn main() {
    let app = gpui_platform::application().with_assets(Assets);

    app.run(move |cx| {
        // Initialize Shilpo UI theme & global states
        shilpo_ui::init(cx);
        cx.activate(true);

        let window_size = size(px(800.), px(56.));
        let options = WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.), px(0.)),
                size: window_size,
            })),
            app_id: Some("shilpo-bar".to_string()),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "bar".to_string(),
                layer: Layer::Top,
                anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                exclusive_zone: Some(px(56.)),
                margin: None,
                keyboard_interactivity: KeyboardInteractivity::None,
                ..Default::default()
            }),
            ..Default::default()
        };

        cx.open_window(options, BarView::view)
            .expect("failed to open status bar window");
    });
}
