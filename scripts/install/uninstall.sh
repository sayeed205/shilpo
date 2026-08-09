#!/usr/bin/env bash
# Uninstallation module for Shilpo desktop binaries and session services

uninstall_shilpo() {
  log "Uninstalling Shilpo binaries and user session services"

  local bin_dir="$HOME/.local/bin"
  local config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
  local data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
  local systemd_user_dir="$config_home/systemd/user"
  local dbus_service_dir="$data_home/dbus-1/services"
  local wants_dir="$systemd_user_dir/niri.service.wants"

  # Stop and disable user services
  if command -v systemctl >/dev/null 2>&1; then
    run systemctl --user disable --now "${SHILPO_USER_UNITS[@]}" || true
  fi

  # Remove niri.service.wants links
  local want_links=()
  local unit
  for unit in "${SHILPO_USER_UNITS[@]}"; do
    want_links+=("$wants_dir/$unit")
  done
  run rm -f "${want_links[@]}"

  # Remove systemd user units & D-Bus service
  local unit_files=()
  for unit in "${SHILPO_USER_UNITS[@]}"; do
    unit_files+=("$systemd_user_dir/$unit")
  done
  unit_files+=(
    "$dbus_service_dir/org.shilpo.Theme.service"
    "$dbus_service_dir/org.shilpo.Device.service"
  )
  run rm -f "${unit_files[@]}"

  # Remove executables
  run rm -f \
    "$bin_dir/shilpo" \
    "$bin_dir/shilpo-shell" \
    "$bin_dir/shilpo-themed" \
    "$bin_dir/shilpo-device-daemon" \
    "$bin_dir/shilpo-settings"

  if command -v systemctl >/dev/null 2>&1; then
    run systemctl --user daemon-reload
  fi

  log "Shilpo desktop binaries and session services removed."
  log "Installed packages, login shell (/usr/bin/fish), SDDM, user configs, wallpapers, and user data were preserved."
}
