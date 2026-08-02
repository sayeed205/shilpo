#!/usr/bin/env bash
# GPU resolution & Arch package installation module

resolve_and_install_packages() {
  log "Resolving system and GPU package contract for Arch Linux"

  # Source dependency arrays
  # shellcheck source=scripts/install/dependencies.sh
  source "$REPO_ROOT/scripts/install/dependencies.sh"

  local -a packages=()
  packages+=("${SHILPO_BUILD_PACKAGES[@]}")
  packages+=("${SHILPO_RUNTIME_PACKAGES[@]}")
  packages+=("${SHILPO_DESKTOP_PACKAGES[@]}")

  # SDDM is a fallback only.  Do not install a second display manager when
  # the machine already has one configured.
  local display_manager_present=false
  if [[ -L /etc/systemd/system/display-manager.service || -e /etc/systemd/system/display-manager.service ]]; then
    display_manager_present=true
  else
    for dm in sddm gdm lightdm ly; do
      if systemctl is-enabled "$dm.service" >/dev/null 2>&1; then
        display_manager_present=true
        break
      fi
    done
  fi
  if ! $display_manager_present; then
    packages+=(sddm)
  fi

  # GPU detection
  local -a gpu_packages=(mesa)
  local has_intel=false
  local has_amd=false
  local has_nvidia=false

  local vendor_file device_file vendor device
  local drm_dir=${SYS_CLASS_DRM_DIR:-/sys/class/drm}
  shopt -s nullglob
  for vendor_file in "$drm_dir"/card*/device/vendor; do
    device_file=$(dirname "$vendor_file")/device
    [[ -f "$vendor_file" && -f "$device_file" ]] || continue

    vendor=$(<"$vendor_file")
    device=$(<"$device_file")
    vendor=$(tr '[:upper:]' '[:lower:]' <<<"$vendor")
    device=$(tr '[:upper:]' '[:lower:]' <<<"$device")

    case "$vendor" in
      0x8086)
        has_intel=true
        gpu_packages+=(vulkan-intel intel-media-driver)
        ;;
      0x1002)
        has_amd=true
        gpu_packages+=(vulkan-radeon)
        ;;
      0x10de)
        has_nvidia=true
        log "NVIDIA GPU device detected: $device"
        local table_file="$REPO_ROOT/data/nvidia/turing_newer_pci_ids.txt"
        if [[ ! -f "$table_file" ]]; then
          error "NVIDIA GPU PCI lookup table missing at $table_file"
          exit 1
        fi

        if ! grep -qi "^$device" "$table_file"; then
          error "NVIDIA GPU device $device is legacy or absent from committed Turing+ hardware table."
          error "Arch Linux official main repositories support Turing (RTX 20xx / GTX 16xx) and newer via open kernel modules."
          error "Pascal and older NVIDIA GPUs require legacy AUR drivers (nvidia-390xx-dkms / nvidia-470xx-dkms)."
          error "Refer to Arch driver transition notice: https://archlinux.org/news/nvidia-590-driver-drops-pascal-support-main-packages-switch-to-open-kernel-modules/"
          exit 1
        fi

        # Detect installed kernel(s)
        local -a kernels=()
        if pacman -Qq linux >/dev/null 2>&1; then kernels+=(linux); fi
        if pacman -Qq linux-lts >/dev/null 2>&1; then kernels+=(linux-lts); fi
        if pacman -Qq linux-zen >/dev/null 2>&1; then kernels+=(linux-zen); fi
        if pacman -Qq linux-hardened >/dev/null 2>&1; then kernels+=(linux-hardened); fi

        if [[ ${#kernels[@]} -eq 1 && ${kernels[0]} == "linux" ]]; then
          gpu_packages+=(nvidia-open nvidia-utils libva-nvidia-driver)
        elif [[ ${#kernels[@]} -eq 1 && ${kernels[0]} == "linux-lts" ]]; then
          gpu_packages+=(nvidia-open-lts nvidia-utils libva-nvidia-driver)
        else
          gpu_packages+=(nvidia-open-dkms nvidia-utils libva-nvidia-driver)
          for k in "${kernels[@]}"; do
            gpu_packages+=("${k}-headers")
          done
        fi
        ;;
    esac
  done
  shopt -u nullglob

  # Hybrid graphics handling
  if $has_nvidia && ($has_intel || $has_amd); then
    log "Hybrid GPU setup detected; adding nvidia-prime"
    gpu_packages+=(nvidia-prime)
  fi

  # Deduplicate package array
  readarray -t packages < <(printf '%s\n' "${packages[@]}" "${gpu_packages[@]}" | sort -u)

  log "Selected ${#packages[@]} packages for installation"

  local elevate
  elevate=$(sudo_command)
  local -a root=()
  [[ -n $elevate ]] && root=("$elevate")

  local -a flags=(-Syu --needed)
  if [[ "${ASSUME_YES:-false}" == "true" ]]; then
    flags+=(--noconfirm)
  fi

  run "${root[@]}" pacman "${flags[@]}" "${packages[@]}"
}
