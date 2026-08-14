# SDK component fixture

This guest component is authored using `shilpo-ext-sdk` to prove end-to-end integration across the canonical `shilpo:extension@0.1.0` component boundary.

The `shilpo-ext-runtime` build script regenerates this ignored artifact when
its integration tests are compiled. To build it manually:

```bash
cargo build \
  --manifest-path desktop/ext-runtime/tests/fixtures/sdk-component/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

The generated `target/` output is intentionally ignored and must not be committed.
