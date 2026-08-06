# Shilpo Assets (Internal)

Internal default asset bundle used for building the Shilpo Storybook and development demonstrations.

> **Note**: `shilpo-assets` is an internal crate and is **not published** to crates.io. Applications using `shilpo-ui`
> are expected to bring their own asset loader and SVG icon assets via GPUI's asset system or custom icon enums.

## Assets & SVG Usage

In GPUI applications, assets (such as SVGs and images) are supplied by implementing GPUI's `AssetSource` trait (for
example, using `rust-embed`) and attaching it during application startup:

```rust,no_run
use gpui::AssetSource;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct AppAssets;

fn main() {
    let app = gpui_platform::application().with_assets(AppAssets);
    // ...
}
```

## Icon Source & License

The default SVG icons bundled in this crate are sourced
from [Google Material Symbols / Icons](https://fonts.google.com/icons), licensed under
the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).

## License

Apache-2.0
