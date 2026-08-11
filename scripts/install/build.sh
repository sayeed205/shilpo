#!/usr/bin/env bash
# Release and weather extension Rust build module

ensure_rustup() {
  if command -v rustup >/dev/null 2>&1; then
    return 0
  fi
  if [[ "${DRY_RUN:-false}" == "true" ]]; then
    log "Would install rustup toolchain"
    return 0
  fi
  command -v curl >/dev/null 2>&1 || {
    error "rustup is missing and curl is unavailable"
    exit 1
  }
  log "Installing Rust toolchain via rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  export PATH="$HOME/.cargo/bin:$PATH"
}

build_release() {
  ensure_rustup
  run rustup toolchain install stable
  run rustup target add --toolchain stable wasm32-wasip2

  local build_dir="${SHILPO_BUILD_DIR:-$REPO_ROOT/target}"
  local extension_target_dir="${SHILPO_EXTENSION_TARGET_DIR:-$REPO_ROOT/extensions/target}"
  export SHILPO_RELEASE_DIR="$build_dir/release"
  export SHILPO_WEATHER_WASM="$extension_target_dir/wasm32-wasip2/release/shilpo_weather_extension.wasm"

  log "Building Shilpo release workspace binaries"
  run env CARGO_TARGET_DIR="$build_dir" cargo +stable build --locked --release \
    -p shilpo

  log "Building bundled weather WASM extension"
  run env CARGO_TARGET_DIR="$extension_target_dir" cargo +stable build --locked --manifest-path extensions/Cargo.toml \
    -p shilpo-weather-extension --target wasm32-wasip2 --release

  local weather_destination="${SHILPO_WEATHER_DESTINATION:-$REPO_ROOT/extensions/weather/extension.wasm}"
  run install -Dm644 "$SHILPO_WEATHER_WASM" "$weather_destination"
}
