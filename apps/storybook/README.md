# Shilpo Storybook

An interactive desktop gallery for exploring, testing, and reviewing all UI components built in `shilpo-ui`.

---

## Running Storybook

To start the interactive Storybook application:

```bash
cargo run -p storybook
```

You can also run a specific component story directly by passing its name:

```bash
cargo run -p storybook -- switch
```

---

## How to Use `shilpo-ui` & Assets in Your Own App

> **Asset Note**: Storybook owns its GPUI asset source and embeds the canonical SVGs from `core/assets/icons/`. Applications using `shilpo-ui` must register their own asset source; the UI crate publishes icon names, not default icon bytes.

### 1. Add Dependencies to `Cargo.toml`

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed" }
shilpo-ui = { git = "https://github.com/sayeed205/shilpo" }
rust-embed = "8"
```

### 2. Define Your Application Asset Source (SVG Icons & Fonts)

Use `rust-embed` to bundle your application's SVG icons and fonts:

```rust,no_run
use gpui::{AssetSource, SharedString};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Self::get(path)
            .map(|f| Some(f.data))
            .ok_or_else(|| anyhow::anyhow!("asset not found: {}", path))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter(|p| p.starts_with(path))
            .map(|p| SharedString::from(p.to_string()))
            .collect())
    }
}
```

### 3. Generate Custom Icon Types with `icon_named!`

`shilpo-ui` provides the `icon_named!` macro to generate strongly-typed icon enums directly from your local SVG directory:

```rust,no_run
use shilpo_ui::icon_named;

// Generate `MyIcon` enum from SVG files in "assets/icons"
icon_named!(MyIcon, "assets/icons");
```

### 4. Initialize Shilpo UI in Your Application

```rust,no_run
use gpui::{App, WindowOptions, px, size};
use shilpo_ui::{
    button::Button,
    switch::Switch,
    Icon, ActiveTheme, Root,
    v_flex, h_flex,
};

fn main() {
    // 1. Pass your application's asset source to GPUI
    let app = gpui_platform::application().with_assets(AppAssets);

    app.run(move |cx| {
        // 2. Initialize Shilpo UI state
        shilpo_ui::init(cx);
        cx.activate(true);

        // 3. Open a window wrapped with Shilpo's Root component
        let options = WindowOptions {
            bounds: Some(gpui::Bounds::centered(None, size(px(800.), px(600.)), cx)),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            Root::new(
                window,
                cx,
                v_flex()
                    .size_full()
                    .p_6()
                    .gap_4()
                    .child(
                        h_flex().gap_4().child(
                            Button::new("btn-demo")
                                .label("Click Me")
                                .icon(MyIcon::Check)
                                .primary(),
                        )
                    )
                    .child(
                        Switch::new("switch-demo")
                            .label("Enable Feature")
                            .show_icons(true),
                    ),
            )
        })
        .unwrap();
    });
}
```

---

## License

Licensed under the [Apache License, Version 2.0](../../LICENSE).
