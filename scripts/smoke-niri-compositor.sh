#!/usr/bin/env bash
set -euo pipefail

# Validate the Shilpo compositor broker against a live Niri session.
# Queries use niri as the state oracle; every mutation goes through shilpo-shell.

MODE=dry-run
for arg in "$@"; do
    case "$arg" in
        --dry-run) MODE=dry-run ;;
        --execute) MODE=execute ;;
        --interactive) MODE=interactive ;;
        -h|--help)
            cat <<'USAGE'
Usage: smoke-niri-compositor.sh [--dry-run|--execute|--interactive]

  --dry-run      Check prerequisites and print the test matrix (default).
  --execute      Run reversible broker/IPC mutations and restore focus/window state.
  --interactive  Verify one bar click and one configured keyboard action via polling.
USAGE
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

NIRI_BIN=${NIRI_BIN:-niri}
if [ -z "${SHILPO_SHELL_BIN:-}" ]; then
    if [ -x target/debug/shilpo-shell ]; then
        SHILPO_SHELL_BIN=target/debug/shilpo-shell
    elif [ -x target/release/shilpo-shell ]; then
        SHILPO_SHELL_BIN=target/release/shilpo-shell
    else
        SHILPO_SHELL_BIN=target/debug/shilpo-shell
    fi
fi
SOCKET_PATH=${NIRI_SOCKET:-${NIRI_SOCKET_PATH:-}}
passed=0
failed=0
skipped=0

report() {
    case "$1" in
        PASS) passed=$((passed + 1)) ;;
        FAIL) failed=$((failed + 1)) ;;
        SKIP) skipped=$((skipped + 1)) ;;
    esac
    printf '[%s] %s\n' "$1" "$2"
}

summary() {
    printf 'Summary: %d passed, %d failed, %d skipped\n' "$passed" "$failed" "$skipped"
}

echo "=== Shilpo Niri compositor smoke test ($MODE) ==="

if command -v "$NIRI_BIN" >/dev/null 2>&1; then
    report PASS "Niri CLI available"
else
    report SKIP "Niri CLI unavailable"
fi

if command -v jq >/dev/null 2>&1; then
    report PASS "jq available"
else
    report SKIP "jq unavailable"
fi

if [ -x "$SHILPO_SHELL_BIN" ]; then
    report PASS "Shilpo shell binary available ($SHILPO_SHELL_BIN)"
else
    report SKIP "Shilpo shell binary unavailable ($SHILPO_SHELL_BIN)"
fi

if [ -n "$SOCKET_PATH" ] && [ -S "$SOCKET_PATH" ]; then
    report PASS "Niri socket available ($SOCKET_PATH)"
else
    report SKIP "Niri socket unavailable"
fi

if [ "$MODE" = dry-run ]; then
    cat <<'MATRIX'
Planned checks:
  1. Shell readiness through: shilpo-shell msg get-status
  2. Workspace focus through Shilpo, verified with niri state queries
  3. Window focus and previous-window through Shilpo
  4. Empty dynamic workspace activation through Shilpo create-workspace
  5. Move-window-to-workspace through Shilpo, followed by restoration
  6. Invalid command IDs are rejected or leave focus unchanged
MATRIX
    summary
    exit 0
fi

if ! command -v "$NIRI_BIN" >/dev/null 2>&1 || ! command -v jq >/dev/null 2>&1 \
    || [ ! -x "$SHILPO_SHELL_BIN" ] || [ -z "$SOCKET_PATH" ] || [ ! -S "$SOCKET_PATH" ]; then
    echo "Live prerequisites are unavailable; no mutations were attempted."
    summary
    exit 0
fi

if ! "$SHILPO_SHELL_BIN" msg get-status >/dev/null 2>&1; then
    report FAIL "Shilpo shell IPC/readiness check"
    summary
    exit 1
fi
report PASS "Shilpo shell IPC/readiness check"

focused_workspace() {
    "$NIRI_BIN" msg -j workspaces | jq -r '.[] | select(.is_focused == true) | .id' | head -n1
}

focused_window() {
    "$NIRI_BIN" msg -j windows | jq -r '.[] | select(.is_focused == true) | .id' | head -n1
}

window_workspace() {
    "$NIRI_BIN" msg -j windows \
        | jq -r --arg id "$1" '.[] | select((.id | tostring) == $id) | (.workspace_id // "")' \
        | head -n1
}

wait_for_window_workspace() {
    local window_id=$1
    local expected_workspace=$2
    for _ in $(seq 1 30); do
        if [ "$(window_workspace "$window_id")" = "$expected_workspace" ]; then
            return 0
        fi
        sleep 0.1
    done
    return 1
}

initial_workspace=$(focused_workspace)
initial_window=$(focused_window || true)
target_workspace=$("$NIRI_BIN" msg -j workspaces \
    | jq -r --arg init "$initial_workspace" '.[] | select((.id | tostring) != $init) | .id' \
    | head -n1)

restore_state() {
    if [ -n "${initial_window:-}" ] && [ "${initial_window:-}" != null ] && [ -n "${initial_workspace:-}" ]; then
        "$SHILPO_SHELL_BIN" msg move-window-to-workspace "$initial_window" "$initial_workspace" >/dev/null 2>&1 || true
    fi
    if [ -n "${initial_workspace:-}" ] && [ "$initial_workspace" != null ]; then
        "$SHILPO_SHELL_BIN" msg focus-workspace "$initial_workspace" >/dev/null 2>&1 || true
    fi
    if [ -n "${initial_window:-}" ] && [ "$initial_window" != null ]; then
        "$SHILPO_SHELL_BIN" msg focus-window "$initial_window" >/dev/null 2>&1 || true
    fi
}
trap restore_state EXIT INT TERM

if [ "$MODE" = interactive ]; then
    echo "Click a different workspace in the Shilpo bar, then press Enter."
    read -r
    before=$(focused_workspace)
    for _ in $(seq 1 30); do
        [ "$(focused_workspace)" != "$before" ] && break
        sleep 0.5
    done
    if [ "$(focused_workspace)" != "$before" ]; then
        report PASS "Workspace bar click reached Niri"
    else
        report FAIL "Workspace bar click produced no focus change"
    fi

    echo "Use a configured Shilpo compositor keyboard action, then press Enter."
    read -r
    before=$(focused_workspace)
    for _ in $(seq 1 30); do
        [ "$(focused_workspace)" != "$before" ] && break
        sleep 0.5
    done
    if [ "$(focused_workspace)" != "$before" ]; then
        report PASS "Keyboard action reached Niri"
    else
        report FAIL "Keyboard action produced no focus change"
    fi
    summary
    exit "$((failed > 0 ? 1 : 0))"
fi

if [ -n "$target_workspace" ] && [ "$target_workspace" != null ]; then
    if "$SHILPO_SHELL_BIN" msg focus-workspace "$target_workspace" >/dev/null 2>&1 \
        && [ "$(focused_workspace)" = "$target_workspace" ]; then
        report PASS "Workspace focus through Shilpo broker"
    else
        report FAIL "Workspace focus through Shilpo broker"
    fi
else
    report SKIP "No secondary workspace available for focus test"
fi

windows_json=$("$NIRI_BIN" msg -j windows)
window_count=$(echo "$windows_json" | jq 'length')
if [ "$window_count" -ge 2 ]; then
    win1=$(echo "$windows_json" | jq -r '.[0].id')
    win2=$(echo "$windows_json" | jq -r '.[1].id')
    if "$SHILPO_SHELL_BIN" msg focus-window "$win1" >/dev/null 2>&1 \
        && "$SHILPO_SHELL_BIN" msg focus-window "$win2" >/dev/null 2>&1 \
        && "$SHILPO_SHELL_BIN" msg focus-previous-window >/dev/null 2>&1 \
        && [ "$(focused_window)" = "$win1" ]; then
        report PASS "Window focus and previous-window through Shilpo broker"
    else
        report FAIL "Window focus and previous-window through Shilpo broker"
    fi
else
    report SKIP "Fewer than two windows available for focus test"
fi

if "$SHILPO_SHELL_BIN" msg create-workspace >/dev/null 2>&1; then
    report PASS "Dynamic empty workspace activation through Shilpo broker"
else
    report FAIL "Dynamic empty workspace activation through Shilpo broker"
fi

if [ -n "${initial_window:-}" ] && [ -n "$target_workspace" ] && [ "$target_workspace" != null ]; then
    if "$SHILPO_SHELL_BIN" msg move-window-to-workspace "$initial_window" "$target_workspace" >/dev/null 2>&1 \
        && wait_for_window_workspace "$initial_window" "$target_workspace"; then
        report PASS "Window move through Shilpo broker"
    else
        report FAIL "Window move through Shilpo broker"
    fi
else
    report SKIP "Focused window or secondary workspace unavailable for move test"
fi

invalid_workspace=$("$NIRI_BIN" msg -j workspaces | jq -r 'if length == 0 then 999999 else ((map(.id) | max) + 1) end')
before_invalid_workspace=$(focused_workspace)
if "$SHILPO_SHELL_BIN" msg focus-workspace "$invalid_workspace" >/dev/null 2>&1; then
    # Niri may acknowledge an unknown action target as Handled.  In that case
    # the safety invariant is that focus does not move to a different target.
    if [ "$(focused_workspace)" = "$before_invalid_workspace" ]; then
        report PASS "Invalid workspace leaves focus unchanged"
    else
        report FAIL "Invalid workspace changed focus"
    fi
else
    report PASS "Invalid workspace rejected by broker"
fi

invalid_window=$("$NIRI_BIN" msg -j windows | jq -r 'if length == 0 then 999999 else ((map(.id) | max) + 1) end')
before_invalid_window=$(focused_window)
if "$SHILPO_SHELL_BIN" msg focus-window "$invalid_window" >/dev/null 2>&1; then
    if [ "$(focused_window)" = "$before_invalid_window" ]; then
        report PASS "Invalid window leaves focus unchanged"
    else
        report FAIL "Invalid window changed focus"
    fi
else
    report PASS "Invalid window rejected by broker"
fi

summary
exit "$((failed > 0 ? 1 : 0))"
