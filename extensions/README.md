# Official Shilpo Extensions

Each directory is an independently packaged Shilpo extension crate. The nested workspace keeps WASI guest builds out of
the main native workspace while sharing a lockfile for reproducible official builds.

Build all official extension guests:

```bash
cargo build --manifest-path extensions/Cargo.toml --workspace \
  --target wasm32-wasip2 --release
```

Each extension README documents the command that copies its component beside `extension.toml`, validates the package,
and runs it in development mode. Official trust is assigned only when the release is signed and published through
Shilpo's official registry; living in this source directory does not grant permissions or trust.

