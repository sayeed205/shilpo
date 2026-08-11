# World Clock Extension Example for Shilpo

This buildable WASI Preview 2 component exercises:

- bar and desktop widget contributions;
- a schema-generated settings page;
- the `palette_generated` event;
- view invalidation and notification effects.

Build the guest and place the component beside the manifest:

```bash
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/world_clock_extension.wasm extension.wasm
```

From the Shilpo repository, validate and package it:

```bash
cargo run -p shilpo-cli -- ext check examples/world-clock
cargo run -p shilpo-cli -- ext pack examples/world-clock
```
