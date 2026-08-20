#!/usr/bin/env bash
# Release Rust build module

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

  local build_dir="${SHILPO_BUILD_DIR:-$REPO_ROOT/target}"
  export SHILPO_RELEASE_DIR="$build_dir/release"

  log "Building Shilpo release workspace binaries"
  run env CARGO_TARGET_DIR="$build_dir" cargo +stable build --locked --release \
    -p shilpo
}
