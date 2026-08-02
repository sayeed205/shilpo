#!/usr/bin/env bash
# Paru AUR helper bootstrap module

ensure_paru() {
  if command -v paru >/dev/null 2>&1; then
    log "Paru AUR helper is already installed"
    return 0
  fi

  if [[ "${DRY_RUN:-false}" == "true" ]]; then
    log "Would bootstrap Paru AUR helper from https://aur.archlinux.org/paru.git"
    return 0
  fi

  log "Bootstrapping Paru AUR helper from official AUR repository"

  local elevate
  elevate=$(sudo_command)
  local -a root=()
  [[ -n $elevate ]] && root=("$elevate")

  local -a flags=(-Syu --needed)
  if [[ "${ASSUME_YES:-false}" == "true" ]]; then
    flags+=(--noconfirm)
  fi
  run "${root[@]}" pacman "${flags[@]}" base-devel git

  local tmp_dir
  tmp_dir=$(mktemp -d)
  trap 'rm -rf "$tmp_dir"' RETURN

  run git clone https://aur.archlinux.org/paru.git "$tmp_dir/paru"
  (
    cd "$tmp_dir/paru" || exit 1
    local -a makepkg_flags=(-si)
    if [[ "${ASSUME_YES:-false}" == "true" ]]; then
      makepkg_flags+=(--noconfirm)
    fi
    run makepkg "${makepkg_flags[@]}"
  )

  if command -v paru >/dev/null 2>&1; then
    log "Paru AUR helper bootstrapped successfully"
  else
    error "Paru installation completed but 'paru' executable is missing"
    exit 1
  fi
}
