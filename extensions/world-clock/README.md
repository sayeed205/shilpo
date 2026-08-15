# World Clock Extension (Experimental)

> **Status**: Experimental Official Extension. This extension serves as an experimental reference for Rust-based WASI Preview 2 components. For the canonical comprehensive showcase, see [`extensions/example`](../example).

This WASI Preview 2 component exercises:

- bar and desktop widget contributions;
- a schema-generated settings page;
- the `palette_generated` event subscription;
- view rendering and notification effects.

## Building

Build the guest WebAssembly component:

```bash
rustup target add wasm32-wasip2
cargo build --manifest-path extensions/Cargo.toml --package world-clock-extension --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/world_clock_extension.wasm extension.wasm
```

## Validation & Packaging

From the Shilpo repository root, validate and package the extension:

```bash
shilpo ext check extensions/world-clock
shilpo ext pack extensions/world-clock
```

See [Shilpo Extension Documentation](../../docs/extensions/index.md) for full architecture and authoring guides.
