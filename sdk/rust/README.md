# Shilpo Rust Extension SDK (`shilpo-ext-sdk`)

[![Crates.io](https://img.shields.io/crates/v/shilpo-ext-sdk.svg)](https://crates.io/crates/shilpo-ext-sdk)
[![Documentation](https://docs.rs/shilpo-ext-sdk/badge.svg)](https://docs.rs/shilpo-ext-sdk)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Official Rust SDK for developing sandboxed WebAssembly extensions for the [Shilpo](https://github.com/shilpo-rs/shilpo) desktop environment.

---

## Features

- **Declarative ViewTree Builders**: Fluent constructors for all 18 canonical UI node types (`row`, `column`, `stack`, `grid`, `text`, `icon`, `image`, `button`, `icon_button`, `toggle`, `slider`, `text_input`, `list`, `spacer`, `divider`, `badge`, `progress`, `loading_indicator`).
- **Ergonomic `view!` Macro**: Declarative UI composition with support for nested children, conditionals, and iterators.
- **Typed Lifecycle Adapter**: Simple [`Extension`] trait and [`export_extension!`] macro wrapping the canonical `shilpo:extension@0.1.0` WIT contract.
- **Durable State Helpers**: Key-value state storage with atomic watch registration snapshots via [`State`].
- **Zero Desktop Dependencies**: Pure cross-platform SDK targeting WebAssembly Component Model (`wasm32-wasip2`).

---

## Quickstart

Add `shilpo-ext-sdk` to your extension's `Cargo.toml`:

```toml
[package]
name = "my-shilpo-extension"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
shilpo-ext-sdk = "0.1.0"
```

### Minimal Extension (`src/lib.rs`)

```rust
use shilpo_ext_sdk::prelude::*;

#[derive(Default)]
struct CounterExtension {
    count: i64,
}

impl Extension for CounterExtension {
    fn activate(&mut self, _activation: Activation) -> Result<(), Error> {
        // Read initial persisted state
        if let Ok(Some(val)) = State::read("count") {
            if let Some(c) = val.as_int() {
                self.count = c;
            }
        }
        Ok(())
    }

    fn on_event(&mut self, event: ExtensionEvent) -> Result<(), Error> {
        if let ExtensionEvent::Input(input) = event {
            if input.event_id == "increment" {
                self.count += 1;
                let _ = State::write("count", self.count);
            }
        }
        Ok(())
    }

    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
        if contribution_id != "counter_widget" {
            return Ok(None);
        }

        Ok(Some(view! {
            row {
                icon("counter").size(16.0),
                text(format!("Count: {}", self.count)).bold(true),
                button("+1", "increment"),
            }
        }))
    }
}

export_extension!(CounterExtension);
```

---

## Building and Development

### Compile to WebAssembly Component

Build your extension using `cargo component` or `cargo`:

```bash
cargo component build --release
```

Or using Cargo with the `wasm32-wasip2` target:

```bash
cargo build --target wasm32-wasip2 --release
```

### Shilpo CLI Workflow

Use the Shilpo developer CLI for project scaffolding, validation, and hot-reload:

```bash
# Build the extension artifact
shilpo ext build

# Check manifest, permissions, and component validity
shilpo ext check

# Start the interactive hot-reload dev server
shilpo ext dev
```

---

## UI Composition Examples

### Fluent Builders

```rust
use shilpo_ext_sdk::prelude::*;

let tree = row()
    .gap(12.0)
    .align_items(Alignment::Center)
    .style(
        style()
            .padding(8.0)
            .corner_radius(12.0)
            .background(SemanticColorToken::SurfaceContainer)
    )
    .child(icon("weather-sunny").size(20.0))
    .child(text("72°F Sunny").font_size(14.0).bold(true))
    .child(button("Refresh", "refresh-weather"))
    .build();
```

### `view!` Macro with Conditionals and Loops

```rust
use shilpo_ext_sdk::prelude::*;

let is_connected = true;
let devices = vec!["Headphones", "Keyboard", "Mouse"];

let tree = view! {
    column {
        row {
            icon("bluetooth"),
            text("Bluetooth Devices").bold(true),
            if is_connected {
                badge("Connected"),
            } else {
                badge("Disconnected"),
            },
        },
        divider(),
        for device in (devices) {
            row {
                icon("bluetooth-connected").size(14.0),
                text(device),
            },
        },
    }
};
```

---

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
