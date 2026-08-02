#!/usr/bin/env bash
# Shilpo installer common utilities

log() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
error() { printf 'error: %s\n' "$*" >&2; }

# Keep the installer-owned user-unit inventory in one place.  The same names
# are used when wiring, validating, and uninstalling a Shilpo session.
SHILPO_USER_UNITS=(
  shilpo-shell.service
  shilpo-themed.service
  shilpo-wallpaper.service
  shilpo-polkit-agent.service
  shilpo-network-agent.service
  shilpo-keyring.service
  shilpo-swayidle.service
  shilpo-first-login.service
)

SHILPO_SESSION_UNITS=(
  shilpo-shell.service
  shilpo-themed.service
  shilpo-wallpaper.service
  shilpo-polkit-agent.service
  shilpo-network-agent.service
  shilpo-keyring.service
  shilpo-swayidle.service
)

run() {
  if [[ "${DRY_RUN:-false}" == "true" ]]; then
    printf '+ %s\n' "$*"
  else
    "$@"
  fi
}

sudo_command() {
  if [[ $(id -u) -eq 0 ]]; then
    printf '%s\n' ""
  elif command -v sudo >/dev/null 2>&1; then
    printf '%s\n' sudo
  elif command -v doas >/dev/null 2>&1; then
    printf '%s\n' doas
  else
    error "Privilege elevation requires sudo or doas"
    exit 1
  fi
}

# Safe template rendering without unsafe sed interpolation.
# Replaces exact placeholder strings safely.
render_template() {
  local src=$1
  local dst=$2
  shift 2
  # Arguments in pairs: key value key value ...
  local content
  content=$(<"$src")
  while [[ $# -ge 2 ]]; do
    local k=$1
    local v=$2
    shift 2
    content="${content//$k/$v}"
  done
  mkdir -p "$(dirname "$dst")"
  printf '%s\n' "$content" >"$dst"
}
