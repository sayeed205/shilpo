use gpui::{App, Bounds, WindowBounds, WindowKind, WindowOptions, point, px, size};
use shilpo_assets::Assets;
use shilpo_settings::SettingsView;

fn main() {
    let app = gpui_platform::application().with_assets(Assets);

    app.run(move |cx: &mut App| {
        shilpo_ui::init(cx);
        if let Some(state) = shilpo_services::WallpaperService::read_current() {
            shilpo_ui::Theme::global_mut(cx).set_source_argb(state.source_argb);
        }

        let rx = shilpo_services::WallpaperService::subscribe();
        cx.spawn(async move |cx| {
            while let Ok(changed) = rx.recv().await {
                cx.update(|cx| {
                    shilpo_ui::Theme::global_mut(cx).set_source_argb(changed.source_argb);
                    cx.refresh_windows();
                });
            }
        })
        .detach();
        cx.activate(true);

        let display_bounds = cx
            .primary_display()
            .map(|d| d.bounds())
            .unwrap_or_else(|| Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))));

        let width = px(900.);
        let height = px(640.);
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - width) / 2.0,
            display_bounds.origin.y + (display_bounds.size.height - height) / 2.0,
        );

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                origin,
                size(width, height),
            ))),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Shilpo Settings".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            kind: WindowKind::Normal,
            display_id: cx.primary_display().map(|d| d.id()),
            window_background: gpui::WindowBackgroundAppearance::Opaque,
            ..Default::default()
        };

        if let Err(err) = cx.open_window(options, SettingsView::view) {
            eprintln!("Failed to open Settings window: {}", err);
        }
    });
}
