#!/usr/bin/env bash
# Staging and atomic commit module for Shilpo desktop configuration and binaries

stage_and_commit_files() {
  local staging_dir
  staging_dir=$(mktemp -d)
  trap 'rm -rf "$staging_dir"' RETURN

  log "Rendering and staging desktop configuration into $staging_dir"

  local bin_dir="$HOME/.local/bin"
  local config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
  local data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
  local state_home="${XDG_STATE_HOME:-$HOME/.local/state}"
  local systemd_user_dir="$config_home/systemd/user"
  local dbus_service_dir="$data_home/dbus-1/services"

  # Stage binaries only for a real install.  A dry-run must remain useful on a
  # clean checkout where release artifacts have not been built yet.
  mkdir -p "$staging_dir/bin"
  if [[ "${DRY_RUN:-false}" == "false" ]]; then
    local release_dir="${SHILPO_RELEASE_DIR:-$REPO_ROOT/target/release}"
    cp -a "$release_dir/shilpo" "$staging_dir/bin/shilpo"
    cp -a "$release_dir/shilpo-shell" "$staging_dir/bin/shilpo-shell"
    cp -a "$release_dir/shilpo-themed" "$staging_dir/bin/shilpo-themed"
    cp -a "$release_dir/shilpo-settings" "$staging_dir/bin/shilpo-settings"
    chmod +x "$staging_dir/bin/"*
  fi

  # Stage systemd services with absolute binary paths
  mkdir -p "$staging_dir/systemd/user"
  render_template data/systemd/user/shilpo-shell.service "$staging_dir/systemd/user/shilpo-shell.service" "/usr/bin/shilpo-shell" "$bin_dir/shilpo-shell"
  render_template data/systemd/user/shilpo-themed.service "$staging_dir/systemd/user/shilpo-themed.service" "/usr/bin/shilpo-themed" "$bin_dir/shilpo-themed"
  render_template data/systemd/user/shilpo-wallpaper.service "$staging_dir/systemd/user/shilpo-wallpaper.service" "/usr/bin/awww-daemon" "/usr/bin/awww-daemon"
  render_template data/systemd/user/shilpo-swayidle.service "$staging_dir/systemd/user/shilpo-swayidle.service" "/usr/bin/swayidle" "/usr/bin/swayidle"
  render_template data/systemd/user/shilpo-first-login.service "$staging_dir/systemd/user/shilpo-first-login.service" \
    "/usr/bin/shilpo" "$bin_dir/shilpo" \
    "@FIRST_LOGIN_MARKER@" "$state_home/shilpo/first-login-completed"
  render_template data/systemd/user/shilpo-network-agent.service "$staging_dir/systemd/user/shilpo-network-agent.service" "/usr/bin/nm-applet" "/usr/bin/nm-applet"
  render_template data/systemd/user/shilpo-keyring.service "$staging_dir/systemd/user/shilpo-keyring.service" "/usr/bin/gnome-keyring-daemon" "/usr/bin/gnome-keyring-daemon"

  # Detect polkit agent executable
  local polkit_agent=""
  for candidate in \
    /usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1 \
    /usr/libexec/polkit-gnome-authentication-agent-1; do
    if [[ -x "$candidate" ]]; then
      polkit_agent=$candidate
      break
    fi
  done
  if [[ -z "$polkit_agent" ]]; then
    polkit_agent=/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1
  fi
  render_template data/systemd/user/shilpo-polkit-agent.service "$staging_dir/systemd/user/shilpo-polkit-agent.service" "@POLKIT_AGENT@" "$polkit_agent"

  # Stage D-Bus service
  mkdir -p "$staging_dir/dbus-1/services"
  render_template data/dbus-1/services/org.shilpo.Theme.service "$staging_dir/dbus-1/services/org.shilpo.Theme.service" "/usr/bin/shilpo-themed" "$bin_dir/shilpo-themed"

  # Stage Niri config tree
  mkdir -p "$staging_dir/niri"
  cp -a data/niri/config.kdl "$staging_dir/niri/config.kdl"
  cp -a data/niri/config.d "$staging_dir/niri/config.d"

  # Render the absolute Shilpo binary path without shell/sed interpolation.
  render_template "$staging_dir/niri/config.d/70-binds.kdl" "$staging_dir/niri/config.d/70-binds.kdl" \
    'spawn "shilpo"' "spawn \"$bin_dir/shilpo\""

  # Stage Kitty, Starship, Swaylock, Swayidle, Fish, Shilpo configs
  mkdir -p "$staging_dir/kitty" "$staging_dir/starship" "$staging_dir/swaylock" "$staging_dir/swayidle" "$staging_dir/fish" "$staging_dir/shilpo" "$staging_dir/wallpapers"
  cp -a data/kitty/kitty.conf "$staging_dir/kitty/kitty.conf"
  cp -a data/starship/starship.toml "$staging_dir/starship/starship.toml"
  cp -a data/swaylock/config "$staging_dir/swaylock/config"
  cp -a data/swayidle/config "$staging_dir/swayidle/config"
  cp -a data/fish/shilpo.fish "$staging_dir/fish/shilpo.fish"
  cp -a data/shilpo/config.toml "$staging_dir/shilpo/config.toml"
  if [[ -f data/wallpapers/shilpo-default.png ]]; then
    cp -a data/wallpapers/shilpo-default.png "$staging_dir/wallpapers/shilpo-default.png"
  fi

  log "Validating staged configuration files and binaries"

  # Validate Niri KDL
  if command -v niri >/dev/null 2>&1; then
    run niri validate -c "$staging_dir/niri/config.kdl"
  fi

  # Validate binaries exist and are executable
  if [[ "${DRY_RUN:-false}" == "false" ]]; then
    for b in shilpo shilpo-shell shilpo-themed shilpo-settings; do
      if [[ ! -x "$staging_dir/bin/$b" ]]; then
        error "Staged executable $b is invalid or missing"
        exit 1
      fi
    done
  fi

  # Validate systemd service units syntax
  for unit in "$staging_dir/systemd/user/"*.service; do
    if ! grep -q '\[Unit\]' "$unit" || ! grep -q '\[Service\]' "$unit"; then
      error "Staged unit $(basename "$unit") failed basic systemd syntax validation"
      exit 1
    fi
  done

  if [[ "${DRY_RUN:-false}" == "true" ]]; then
    log "Dry run: skipping file commit to live filesystem"
    return 0
  fi

  log "Committing staged desktop files to live configuration paths"

  # 1. Binaries
  mkdir -p "$bin_dir"
  install -Dm755 "$staging_dir/bin/shilpo" "$bin_dir/shilpo"
  install -Dm755 "$staging_dir/bin/shilpo-shell" "$bin_dir/shilpo-shell"
  install -Dm755 "$staging_dir/bin/shilpo-themed" "$bin_dir/shilpo-themed"
  install -Dm755 "$staging_dir/bin/shilpo-settings" "$bin_dir/shilpo-settings"

  # 2. Systemd & D-Bus
  mkdir -p "$systemd_user_dir" "$dbus_service_dir"
  for unit in "$staging_dir/systemd/user/"*.service; do
    install -Dm644 "$unit" "$systemd_user_dir/$(basename "$unit")"
  done
  install -Dm644 "$staging_dir/dbus-1/services/org.shilpo.Theme.service" "$dbus_service_dir/org.shilpo.Theme.service"

  # 3. Niri config tree (authoritative overwrite)
  mkdir -p "$config_home/niri/config.d"
  install -Dm644 "$staging_dir/niri/config.kdl" "$config_home/niri/config.kdl"
  for f in "$staging_dir/niri/config.d/"*.kdl; do
    if [[ $(basename "$f") == "90-user-extra.kdl" && -e "$config_home/niri/config.d/90-user-extra.kdl" ]]; then
      continue
    fi
    install -Dm644 "$f" "$config_home/niri/config.d/$(basename "$f")"
  done

  # 4. Dotfiles & Shilpo config
  install -Dm644 "$staging_dir/kitty/kitty.conf" "$config_home/kitty/kitty.conf"
  install -Dm644 "$staging_dir/starship/starship.toml" "$config_home/starship/starship.toml"
  install -Dm644 "$staging_dir/swaylock/config" "$config_home/swaylock/config"
  install -Dm644 "$staging_dir/swayidle/config" "$config_home/swayidle/config"
  install -Dm644 "$staging_dir/fish/shilpo.fish" "$config_home/fish/conf.d/shilpo.fish"
  install -Dm644 "$staging_dir/shilpo/config.toml" "$config_home/shilpo/config.toml"

  # 5. Wallpapers
  local wall_dir="$HOME/Pictures/Wallpapers"
  mkdir -p "$wall_dir"
  if [[ -f "$staging_dir/wallpapers/shilpo-default.png" && ! -f "$wall_dir/shilpo-default.png" ]]; then
    install -Dm644 "$staging_dir/wallpapers/shilpo-default.png" "$wall_dir/shilpo-default.png"
  fi

  log "Committed authoritative configuration and binaries successfully"

  # Remove only known iNiR/Quickshell-owned artifacts.  Personal browser
  # profiles and unrelated desktop configuration are intentionally untouched.
  if command -v systemctl >/dev/null 2>&1; then
    run systemctl --user disable --now inir.service inir-super-overview.service || true
  fi
  run rm -f \
    "$systemd_user_dir/inir.service" \
    "$systemd_user_dir/inir-super-overview.service" \
    "$systemd_user_dir/niri.service.wants/inir.service" \
    "$systemd_user_dir/niri.service.wants/inir-super-overview.service"
  run rm -rf \
    "$config_home/inir" \
    "$config_home/quickshell" \
    "$config_home/dank-material-shell" \
    "$config_home/darkly" \
    "${XDG_CACHE_HOME:-$HOME/.cache}/quickshell" \
    "$data_home/quickshell"
}
