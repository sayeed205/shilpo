#!/usr/bin/env bash
# Service activation, SDDM setup, login shell, and session wiring module

activate_services_and_shell() {
  log "Wiring systemd user services for Niri session"

  local config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
  local systemd_user_dir="$config_home/systemd/user"
  local wants_dir="$systemd_user_dir/niri.service.wants"

  run mkdir -p "$wants_dir"

  # Link every installer-owned user unit into the Niri session target.
  local u
  for u in "${SHILPO_USER_UNITS[@]}"; do
    if [[ ! -e "$wants_dir/$u" ]]; then
      run ln -sf "$systemd_user_dir/$u" "$wants_dir/$u"
    fi
  done

  # Reload systemd user daemon
  if command -v systemctl >/dev/null 2>&1; then
    run systemctl --user daemon-reload
  fi

  # A wants-link does not start a unit when the Niri session is already
  # running. Start the session consumers now; a fresh login will start them
  # through niri.service.wants instead.
  local active_niri=false
  if [[ -n ${WAYLAND_DISPLAY:-} || -n ${NIRI_SOCKET:-} ]]; then
    active_niri=true
  fi
  if $active_niri && command -v systemctl >/dev/null 2>&1; then
    for u in "${SHILPO_SESSION_UNITS[@]}"; do
      run systemctl --user start "$u" || warn "Could not start user unit $u in the active session"
    done
  fi

  # A minimal Arch install does not enable these system services for us.
  # They are prerequisites for a usable network/Bluetooth desktop session.
  local elevate
  elevate=$(sudo_command)
  local -a root=()
  [[ -n $elevate ]] && root=("$elevate")
  run "${root[@]}" systemctl enable --now NetworkManager.service bluetooth.service

  # Display manager & SDDM setup
  log "Verifying display manager configuration"
  local dm_active=false
  if command -v systemctl >/dev/null 2>&1; then
    if systemctl is-enabled display-manager.service >/dev/null 2>&1 \
      || systemctl is-enabled sddm.service >/dev/null 2>&1 \
      || systemctl is-enabled gdm.service >/dev/null 2>&1 \
      || systemctl is-enabled lightdm.service >/dev/null 2>&1 \
      || systemctl is-enabled ly.service >/dev/null 2>&1; then
      dm_active=true
      log "Preserving existing display manager configuration"
    fi
  fi

  if ! $dm_active; then
    log "No display manager enabled; enabling SDDM for next boot"
    local elevate
    elevate=$(sudo_command)
    local -a root=()
    [[ -n $elevate ]] && root=("$elevate")
    run "${root[@]}" systemctl enable sddm.service
  fi

  # Verify niri wayland session desktop file exists
  if [[ ! -f /usr/share/wayland-sessions/niri.desktop && "${DRY_RUN:-false}" == "false" ]]; then
    warn "/usr/share/wayland-sessions/niri.desktop is missing; verify niri package installation"
  fi

  # Change login shell to Fish
  if [[ -x /usr/bin/fish ]]; then
    local current_shell
    current_shell=$(getent passwd "$USER" | cut -d: -f7)
    if [[ $current_shell != "/usr/bin/fish" ]]; then
      log "Changing login shell to /usr/bin/fish"
      local elevate
      elevate=$(sudo_command)
      if [[ -n $elevate ]]; then
        run "$elevate" chsh -s /usr/bin/fish "$USER"
      else
        run chsh -s /usr/bin/fish
      fi
    fi
  fi

  # XDG user directories
  if command -v xdg-user-dirs-update >/dev/null 2>&1; then
    run xdg-user-dirs-update
  fi
  run mkdir -p "$HOME/Pictures/Screenshots" "$HOME/Pictures/Wallpapers"

  if $active_niri && command -v systemctl >/dev/null 2>&1; then
    run systemctl --user start shilpo-first-login.service || warn "Could not start first-login diagnostics"
  fi

  # Verification: immediately run doctor if in active Niri session
  if [[ -n ${WAYLAND_DISPLAY:-} || -n ${NIRI_SOCKET:-} ]]; then
    log "Active Niri graphical session detected; running immediate verification"
    if [[ -x "$HOME/.local/bin/shilpo" ]]; then
      run "$HOME/.local/bin/shilpo" doctor
    fi
  else
    log "Verification scheduled via shilpo-first-login.service on first Niri login"
  fi
}

prompt_reboot() {
  if [[ "${ASSUME_YES:-false}" == "true" ]]; then
    log "Installation complete (-y specified; suppressing reboot prompt)"
    return 0
  fi

  if [[ "${DRY_RUN:-false}" == "true" ]]; then
    log "Dry run complete"
    return 0
  fi

  printf '\n'
  printf 'Shilpo desktop installation complete!\n'
  read -r -p "Reboot now to start SDDM/Niri desktop? [y/N] " response
  case "$response" in
    [yY][eE][sS]|[yY])
      log "Rebooting system..."
      local elevate
      elevate=$(sudo_command)
      if [[ -n $elevate ]]; then
        exec "$elevate" reboot
      else
        exec reboot
      fi
      ;;
    *)
      log "Reboot skipped. Log into Niri via SDDM to complete setup."
      ;;
  esac
}
