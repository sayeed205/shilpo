#!/usr/bin/env bash
# Preflight environment verification for Arch Linux target

run_preflight() {
  log "Running preflight system verification"

  # 1. Non-root user check
  if [[ $(id -u) -eq 0 ]]; then
    error "Run ./setup as your normal desktop user, not root"
    exit 1
  fi

  # 2. Arch Linux identity check (reject derivatives)
  local os_release_file="${SHILPO_OS_RELEASE:-/etc/os-release}"
  if [[ -f "$os_release_file" ]]; then
    # shellcheck disable=SC1091
    local os_id
    os_id=$(gawk -F= '$1=="ID"{print $2}' "$os_release_file" 2>/dev/null | tr -d '"')
    if [[ $os_id != "arch" ]]; then
      error "Pure Arch Linux is the only supported target (detected ID='$os_id')"
      exit 1
    fi
  else
    error "$os_release_file missing; cannot verify Arch Linux system"
    exit 1
  fi

  # 3. Privilege helper check
  local elevate
  elevate=$(sudo_command)
  if [[ -n $elevate ]]; then
    if ! $elevate -n true 2>/dev/null && [[ "${ASSUME_YES:-false}" == "true" ]]; then
      log "Sudo privileges required for system packages"
    fi
  fi

  # 4. Systemd check
  if ! command -v systemctl >/dev/null 2>&1 || ! systemctl is-system-running >/dev/null 2>&1; then
    if ! systemctl status >/dev/null 2>&1; then
      error "Active systemd init system is required"
      exit 1
    fi
  fi

  # 5. Installed kernel check
  local has_kernel=false
  local boot_dir="${SHILPO_BOOT_DIR:-/boot}"
  if compgen -G "$boot_dir/vmlinuz-*" >/dev/null; then
    has_kernel=true
  else
    local kernel_package
    for kernel_package in linux linux-lts linux-zen linux-hardened; do
      if pacman -Qq "$kernel_package" >/dev/null 2>&1; then
        has_kernel=true
        break
      fi
    done
  fi
  if ! $has_kernel; then
    error "No installed Linux kernel detected in /boot or package database"
    exit 1
  fi

  # 6. Internet connection check
  if ! ping -c 1 -W 2 archlinux.org >/dev/null 2>&1 && ! curl -sI --connect-timeout 3 https://archlinux.org >/dev/null 2>&1; then
    warn "Internet connectivity test unconfirmed; package installation may fail if offline"
  fi

  log "Preflight system checks passed"
}
