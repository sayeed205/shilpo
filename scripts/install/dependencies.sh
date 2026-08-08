#!/usr/bin/env bash
# Auditable Arch Linux desktop dependency inventory for Shilpo.
# Target: Arch Linux exclusive desktop contract.

SHILPO_BUILD_PACKAGES=(
  base-devel clang cmake git pkgconf rustup
  alsa-lib ffmpeg fontconfig glib2 libdrm libva libxcb libxkbcommon-x11
  openssl pipewire sqlite vulkan-icd-loader wayland zstd
)

SHILPO_RUNTIME_PACKAGES=(
  niri systemd dbus networkmanager pipewire pipewire-pulse pipewire-alsa wireplumber
  bluez bluez-utils brightnessctl power-profiles-daemon upower geoclue
  xdg-desktop-portal xdg-desktop-portal-gtk xdg-desktop-portal-gnome xdg-utils xdg-user-dirs
  awww librsvg gtk3 libnotify polkit util-linux vulkan-tools
  tesseract tesseract-data-eng
  noto-fonts noto-fonts-emoji ttf-jetbrains-mono-nerd capitaine-cursors breeze-icons
)

SHILPO_DESKTOP_PACKAGES=(
  linux-firmware sof-firmware alsa-ucm-conf pciutils usbutils
  fish starship kitty nautilus gvfs gvfs-mtp
  polkit-gnome gnome-keyring network-manager-applet playerctl
  xwayland-satellite swaylock swayidle pavucontrol wlsunset
)
