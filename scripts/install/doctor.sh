#!/usr/bin/env bash
# Doctor diagnostic check module

run_doctor() {
  log "Executing Shilpo doctor diagnostic check"

  if [[ -x "$HOME/.local/bin/shilpo" ]]; then
    "$HOME/.local/bin/shilpo" doctor
  else
    error "Shilpo CLI binary missing at $HOME/.local/bin/shilpo; run './setup install' first"
    exit 1
  fi
}
