# Rust Extension Developer Guide

This guide walks you through authoring, building, and packaging a Shilpo desktop extension in Rust using the `wasm32-wasip2` target and `wit-bindgen`.

---

## 1. Prerequisites

Add the `wasm32-wasip2` target to your Rust toolchain:

```bash
rustup target add wasm32-wasip2
```

---

## 2. Scaffolding a New Rust Extension

Use the Shilpo CLI to scaffold a Rust extension project:

```bash
shilpo ext new my-rust-extension --rust --starter bar-widget
```

### Project Layout

```
my-rust-extension/
├── Cargo.toml              # Rust crate manifest configured for cdylib and rlib
├── extension.toml          # Extension manifest (ID, contributions, capabilities)
├── settings.schema.json    # JSON schema for configurable settings
├── src/
│   └── lib.rs              # Rust extension implementation
└── README.md
```

---

## 3. Authoring the Extension in Rust

In `src/lib.rs`, generate WIT bindings targeting `extension` world and implement the `Guest` trait:

```rust
#![cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
mod guest {
    wit_bindgen::generate!({
        path: "wit",
        world: "extension",
    });

    use shilpo::extension::{events, types, view};

    struct MyExtension;

    impl Guest for MyExtension {
        fn activate(_activation: types::Activation) -> Result<(), types::Error> {
            Ok(())
        }

        fn deactivate(_reason: types::DeactivateReason) -> Result<(), types::Error> {
            Ok(())
        }

        fn on_event(_event: events::ExtensionEvent) -> Result<(), types::Error> {
            Ok(())
        }

        fn view(contribution_id: String) -> Result<Option<view::ViewTree>, types::Error> {
            if contribution_id == "my-bar-widget" {
                let node = view::ViewNode::Text(view::TextNode {
                    content: "Hello from Rust!".into(),
                    font_size: Some(14.0),
                    bold: Some(true),
                    style: None,
                });
                return Ok(Some(view::ViewTree {
                    nodes: vec![node],
                    root: 0,
                }));
            }
            Ok(None)
        }
    }

    export!(MyExtension);
}
```

---

## 4. Building the WASI Component

Compile the crate targeting `wasm32-wasip2` in release mode:

```bash
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/my_rust_extension.wasm extension.wasm
```

---

## 5. Validating & Packaging

Validate and package the extension:

```bash
shilpo ext check my-rust-extension
shilpo ext pack my-rust-extension
```

---

## Reference Extensions

- **[`extensions/world-clock`](../../extensions/world-clock)**: Reference Rust implementation showcasing widget contributions, settings integration, and event subscriptions.
