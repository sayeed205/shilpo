# Bar menu component fixture

This guest component is the end-to-end proof fixture for the canonical
`shilpo:extension@0.1.0` boundary. Its checked-in release artifact is consumed
by `desktop/ext-runtime/src/wasm.rs` so ordinary test runs do not require a
WebAssembly Rust target.

Regenerate the artifact after changing this source or the canonical WIT:

```bash
cargo build \
  --manifest-path desktop/ext-runtime/tests/fixtures/bar-menu-component/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

Commit only
`target/wasm32-wasip2/release/bar_menu_component_fixture.wasm` from the generated
target directory.
