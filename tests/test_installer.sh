#!/usr/bin/env bash
# Isolated shell integration test suite for Shilpo Arch installer

set -Eeuo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)

PASSED=0
FAILED=0

assert_eq() {
  local expected=$1
  local actual=$2
  local msg=$3
  if [[ "$expected" == "$actual" ]]; then
    printf '  [✓ PASS] %s\n' "$msg"
    PASSED=$((PASSED + 1))
  else
    printf '  [✗ FAIL] %s (expected "%s", got "%s")\n' "$msg" "$expected" "$actual" >&2
    FAILED=$((FAILED + 1))
  fi
}

assert_file_exists() {
  local file=$1
  local msg=$2
  if [[ -e "$file" ]]; then
    printf '  [✓ PASS] %s\n' "$msg"
    PASSED=$((PASSED + 1))
  else
    printf '  [✗ FAIL] %s (file "%s" does not exist)\n' "$msg" "$file" >&2
    FAILED=$((FAILED + 1))
  fi
}

assert_contains() {
  local haystack=$1
  local needle=$2
  local msg=$3
  if [[ "$haystack" == *"$needle"* ]]; then
    printf '  [✓ PASS] %s\n' "$msg"
    PASSED=$((PASSED + 1))
  else
    printf '  [✗ FAIL] %s ("%s" not found in output)\n' "$msg" "$needle" >&2
    FAILED=$((FAILED + 1))
  fi
}

setup_test_env() {
  TEST_DIR=$(mktemp -d)
  FAKE_HOME="$TEST_DIR/home"
  FAKE_BIN="$TEST_DIR/bin"
  FAKE_ROOT="$TEST_DIR/root"
  mkdir -p "$FAKE_HOME" "$FAKE_BIN" "$FAKE_ROOT/etc" "$FAKE_ROOT/boot" "$FAKE_ROOT/usr/bin" "$FAKE_ROOT/sys/class/drm/card0/device"

  cat >"$FAKE_ROOT/etc/os-release" <<'EOF'
NAME="Arch Linux"
PRETTY_NAME="Arch Linux"
ID=arch
BUILD_ID=rolling
EOF

  touch "$FAKE_ROOT/boot/vmlinuz-linux"

  # Create mock commands in FAKE_BIN
  cat >"$FAKE_BIN/pacman" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == "-Qq" ]]; then
  if [[ $# -eq 2 && "$2" == "linux" ]]; then
    echo "linux"
    exit 0
  fi
  exit 1
fi
echo "[mock pacman] $*"
exit 0
EOF

  cat >"$FAKE_BIN/sudo" <<'EOF'
#!/usr/bin/env bash
"$@"
EOF

  cat >"$FAKE_BIN/systemctl" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == "is-system-running" ]]; then
  echo "running"
  exit 0
fi
if [[ "$1" == "is-enabled" ]]; then
  exit 1
fi
echo "[mock systemctl] $*"
exit 0
EOF

  cat >"$FAKE_BIN/rustup" <<'EOF'
#!/usr/bin/env bash
echo "[mock rustup] $*"
exit 0
EOF

  cat >"$FAKE_BIN/cargo" <<'EOF'
#!/usr/bin/env bash
if [[ "$*" == *"build"* ]]; then
  mkdir -p "$SHILPO_BUILD_DIR/release" "$SHILPO_EXTENSION_TARGET_DIR/wasm32-wasip2/release"
  cat >"$SHILPO_BUILD_DIR/release/shilpo" <<'SCRIPT'
#!/usr/bin/env bash
if [[ "$1" == "doctor" ]]; then
  echo "=== Shilpo Doctor Diagnostics ==="
  exit 0
fi
if [[ "$1" == "ext" && "$2" == "pack" ]]; then
  output_dir=""
  for ((i=1; i<=$#; i++)); do
    if [[ "${!i}" == "--output" ]]; then
      j=$((i + 1))
      output_dir="${!j}"
    fi
  done
  mkdir -p "$output_dir"
  touch "$output_dir/org.shilpo.weather-1.0.0.shilpo-ext"
fi
exit 0
SCRIPT
  chmod +x "$SHILPO_BUILD_DIR/release/shilpo"
  touch "$SHILPO_BUILD_DIR/release/shilpo-shell" "$SHILPO_BUILD_DIR/release/shilpo-themed" "$SHILPO_BUILD_DIR/release/shilpo-settings"
  chmod +x "$SHILPO_BUILD_DIR/release/shilpo-shell" "$SHILPO_BUILD_DIR/release/shilpo-themed" "$SHILPO_BUILD_DIR/release/shilpo-settings"

  touch "$SHILPO_EXTENSION_TARGET_DIR/wasm32-wasip2/release/shilpo_weather_extension.wasm"
fi
echo "[mock cargo] $*"
exit 0
EOF

  cat >"$FAKE_BIN/git" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == "clone" ]]; then
  mkdir -p "${@: -1}"
fi
echo "[mock git] $*"
exit 0
EOF

  cat >"$FAKE_BIN/makepkg" <<'EOF'
#!/usr/bin/env bash
echo "[mock makepkg] $*"
touch "$FAKE_BIN/paru"
chmod +x "$FAKE_BIN/paru"
exit 0
EOF

  cat >"$FAKE_BIN/id" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == "-u" ]]; then
  echo 1000
else
  /usr/bin/id "$@"
fi
EOF

  cat >"$FAKE_BIN/shilpo" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == "ext" && "$2" == "pack" ]]; then
  output_dir=""
  for ((i=1; i<=$#; i++)); do
    if [[ "${!i}" == "--output" ]]; then
      j=$((i + 1))
      output_dir="${!j}"
    fi
  done
  mkdir -p "$output_dir"
  touch "$output_dir/org.shilpo.weather-1.0.0.shilpo-ext"
fi
exit 0
EOF

  cat >"$FAKE_BIN/chsh" <<'EOF'
#!/usr/bin/env bash
echo "[mock chsh] $*"
exit 0
EOF

  cat >"$FAKE_BIN/reboot" <<'EOF'
#!/usr/bin/env bash
echo "[mock reboot] $*"
exit 0
EOF

  cat >"$FAKE_BIN/notify-send" <<'EOF'
#!/usr/bin/env bash
echo "[mock notify-send] $*"
exit 0
EOF

  cat >"$FAKE_BIN/niri" <<'EOF'
#!/usr/bin/env bash
echo "[mock niri] $*"
exit 0
EOF

  cat >"$FAKE_BIN/awww" <<'EOF'
#!/usr/bin/env bash
echo "[mock awww] $*"
exit 0
EOF

  cat >"$FAKE_BIN/vulkaninfo" <<'EOF'
#!/usr/bin/env bash
echo "Vulkan Instance Version: 1.3.290"
exit 0
EOF

  chmod +x "$FAKE_BIN/"*

  # Set mock environment variables
  export HOME="$FAKE_HOME"
  export PATH="$FAKE_BIN:$PATH"
  export XDG_CONFIG_HOME="$FAKE_HOME/.config"
  export XDG_DATA_HOME="$FAKE_HOME/.local/share"
  export XDG_STATE_HOME="$FAKE_HOME/.local/state"
  export XDG_CACHE_HOME="$FAKE_HOME/.cache"
  export SYS_CLASS_DRM_DIR="$FAKE_ROOT/sys/class/drm"
  export SHILPO_OS_RELEASE="$FAKE_ROOT/etc/os-release"
  export SHILPO_BOOT_DIR="$FAKE_ROOT/boot"
  export SHILPO_BUILD_DIR="$TEST_DIR/build"
  export SHILPO_EXTENSION_TARGET_DIR="$TEST_DIR/extensions-target"
  export SHILPO_WEATHER_DESTINATION="$TEST_DIR/weather/extension.wasm"
  export SHILPO_PACKAGE_DIR="$TEST_DIR/packages"
  export FAKE_BIN
}

cleanup_test_env() {
  if [[ -n ${TEST_DIR:-} && -d $TEST_DIR ]]; then
    rm -rf "$TEST_DIR"
  fi
}

printf '==================================================\n'
printf 'Running Shilpo Installer Integration Tests\n'
printf '==================================================\n\n'

# Test 1: Help interface
setup_test_env
output=$("$REPO_ROOT/setup" --help)
assert_contains "$output" "Shilpo Arch Linux Desktop Installer" "Help command displays installer summary"
cleanup_test_env

# Test 2: Dry Run Install (Zero mutations)
setup_test_env
output=$("$REPO_ROOT/setup" install --dry-run -y)
assert_contains "$output" "Preflight system checks passed" "Dry run passes preflight checks"
assert_contains "$output" "sddm" "Dry run selects SDDM only when no display manager is configured"
assert_eq "false" "$([[ -f $FAKE_HOME/.local/bin/shilpo ]] && echo true || echo false)" "Dry run creates zero files in live HOME"
cleanup_test_env

# Test 3: Kernel package detection does not require every kernel flavor
setup_test_env
rm -f "$FAKE_ROOT/boot/vmlinuz-linux"
output=$("$REPO_ROOT/setup" install --dry-run -y)
assert_contains "$output" "Preflight system checks passed" "Accepts one installed kernel flavor without a /boot image"
cleanup_test_env

# Test 4: Fresh Arch TTY Install
setup_test_env
output=$("$REPO_ROOT/setup" install -y)
assert_file_exists "$FAKE_HOME/.local/bin/shilpo" "Installs shilpo binary"
assert_file_exists "$FAKE_HOME/.local/bin/shilpo-shell" "Installs shilpo-shell binary"
assert_file_exists "$FAKE_HOME/.local/bin/shilpo-themed" "Installs shilpo-themed binary"
assert_file_exists "$FAKE_HOME/.config/niri/config.kdl" "Installs Niri config"
  assert_file_exists "$FAKE_HOME/.config/systemd/user/niri.service.wants/shilpo-shell.service" "Wires shilpo-shell service into niri.service.wants"
  assert_file_exists "$FAKE_HOME/.config/systemd/user/niri.service.wants/shilpo-network-agent.service" "Wires NetworkManager agent into niri.service.wants"
  assert_file_exists "$FAKE_HOME/.config/systemd/user/niri.service.wants/shilpo-keyring.service" "Wires GNOME Keyring into niri.service.wants"
  first_login_unit=$(<"$FAKE_HOME/.config/systemd/user/shilpo-first-login.service")
  assert_contains "$first_login_unit" "ConditionPathExists=!$FAKE_HOME/.local/state/shilpo/first-login-completed" "Renders first-login marker path"
assert_file_exists "$FAKE_HOME/.config/fish/conf.d/shilpo.fish" "Installs Fish shell snippet"
assert_file_exists "$FAKE_HOME/.config/shilpo/config.toml" "Installs Shilpo config.toml"
cleanup_test_env

# Test 5: Active Niri Session Install
setup_test_env
export WAYLAND_DISPLAY="wayland-1"
export NIRI_SOCKET="/tmp/niri-test.sock"
output=$("$REPO_ROOT/setup" install -y)
assert_contains "$output" "Active Niri graphical session detected" "Detects active Niri session during install"
cleanup_test_env

# Test 6: Execution outside repository directory
setup_test_env
(
  cd "$TEST_DIR"
  output=$("$REPO_ROOT/setup" install --dry-run -y)
  assert_contains "$output" "Preflight system checks passed" "Execution outside repository root succeeds"
)
cleanup_test_env

# Test 7: Idempotent Update & Authoritative Config Overwrite
setup_test_env
"$REPO_ROOT/setup" install -y >/dev/null
echo "modified config" >"$FAKE_HOME/.config/niri/config.kdl"
output=$("$REPO_ROOT/setup" update -y)
assert_contains "$output" "Committed authoritative configuration" "Authoritative update overwrites Niri config"
config_content=$(<"$FAKE_HOME/.config/niri/config.kdl")
assert_contains "$config_content" "Shilpo" "Niri config restored to authoritative template"
cleanup_test_env

# Test 8: GPU Resolution - Intel & AMD Fixtures
setup_test_env
echo "0x8086" >"$FAKE_ROOT/sys/class/drm/card0/device/vendor"
echo "0x9bc4" >"$FAKE_ROOT/sys/class/drm/card0/device/device"
output=$("$REPO_ROOT/setup" install --dry-run -y)
assert_contains "$output" "vulkan-intel" "Detects Intel GPU and includes vulkan-intel"
cleanup_test_env

# Test 9: GPU Resolution - Turing NVIDIA Fixture
setup_test_env
echo "0x10de" >"$FAKE_ROOT/sys/class/drm/card0/device/vendor"
echo "0x1f08" >"$FAKE_ROOT/sys/class/drm/card0/device/device"
output=$("$REPO_ROOT/setup" install --dry-run -y)
assert_contains "$output" "nvidia-open" "Classifies Turing NVIDIA GPU (0x1f08) and selects open driver"
cleanup_test_env

# Test 10: GPU Resolution - Unsupported/Legacy NVIDIA Fixture
setup_test_env
echo "0x10de" >"$FAKE_ROOT/sys/class/drm/card0/device/vendor"
echo "0x1b80" >"$FAKE_ROOT/sys/class/drm/card0/device/device"
if "$REPO_ROOT/setup" install --dry-run -y 2>"$TEST_DIR/err.log"; then
  printf '  [✗ FAIL] Reject legacy NVIDIA GPU 0x1b80 (expected failure)\n' >&2
  FAILED=$((FAILED + 1))
else
  err_msg=$(<"$TEST_DIR/err.log")
  assert_contains "$err_msg" "legacy or absent" "Rejects legacy Pascal NVIDIA GPU before package mutation"
fi
cleanup_test_env

# Test 11: Doctor Diagnostics Execution
setup_test_env
"$REPO_ROOT/setup" install -y >/dev/null
output=$("$REPO_ROOT/setup" doctor)
assert_contains "$output" "Shilpo Doctor Diagnostics" "Runs doctor diagnostics suite"
cleanup_test_env

# Test 12: Uninstall preserves configurations and packages
setup_test_env
"$REPO_ROOT/setup" install -y >/dev/null
output=$("$REPO_ROOT/setup" uninstall)
assert_eq "false" "$([[ -f $FAKE_HOME/.local/bin/shilpo ]] && echo true || echo false)" "Removes shilpo binary"
assert_file_exists "$FAKE_HOME/.config/niri/config.kdl" "Preserves Niri configuration"
assert_file_exists "$FAKE_HOME/.config/shilpo/config.toml" "Preserves Shilpo configuration"
cleanup_test_env

# Test 13: Dependency contract verification for native capture
setup_test_env
source "$REPO_ROOT/scripts/install/dependencies.sh"
all_pkgs="${SHILPO_BUILD_PACKAGES[*]} ${SHILPO_RUNTIME_PACKAGES[*]} ${SHILPO_DESKTOP_PACKAGES[*]}"
assert_contains "$all_pkgs" "tesseract" "Dependency contract includes tesseract"
assert_contains "$all_pkgs" "libdrm" "Dependency contract includes libdrm"
assert_eq "false" "$([[ "$all_pkgs" == *"ffmpeg"* ]] && echo true || echo false)" "Dependency contract excludes ffmpeg"
assert_eq "false" "$([[ "$all_pkgs" == *"grim"* ]] && echo true || echo false)" "Dependency contract excludes grim"
assert_eq "false" "$([[ "$all_pkgs" == *"slurp"* ]] && echo true || echo false)" "Dependency contract excludes slurp"
assert_eq "false" "$([[ "$all_pkgs" == *"wf-recorder"* ]] && echo true || echo false)" "Dependency contract excludes wf-recorder"
assert_eq "false" "$([[ "$all_pkgs" == *"swappy"* ]] && echo true || echo false)" "Dependency contract excludes swappy"
assert_eq "false" "$([[ "$all_pkgs" == *"wl-clipboard"* ]] && echo true || echo false)" "Dependency contract excludes wl-clipboard"
cleanup_test_env

printf '\n==================================================\n'
printf 'Test Suite Summary: %d Passed, %d Failed\n' "$PASSED" "$FAILED"
printf '==================================================\n'

if [[ $FAILED -gt 0 ]]; then
  exit 1
fi
