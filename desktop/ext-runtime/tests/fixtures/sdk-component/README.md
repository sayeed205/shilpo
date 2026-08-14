# SDK component fixture

This guest component is authored using `shilpo-ext-sdk` to prove end-to-end integration across the canonical `shilpo:extension@0.1.0` component boundary.

Regenerate the release artifact after modifying this source or the SDK:

```bash
cargo build \
  --manifest-path desktop/ext-runtime/tests/fixtures/sdk-component/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

Commit only `target/wasm32-wasip2/release/sdk_component_fixture.wasm` from the generated target directory.
