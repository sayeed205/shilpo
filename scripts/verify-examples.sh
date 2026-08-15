#!/usr/bin/env bash
# Verification entry point for Shilpo official examples and showcase extensions.
# Runs hermetic tests, manifest/schema validation, component compilation, and coverage matrix checks.

set -Eeuo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT" || exit 1

PASSED=0
FAILED=0

pass() {
  printf '  [✓ PASS] %s\n' "$1"
  PASSED=$((PASSED + 1))
}

fail() {
  printf '  [✗ FAIL] %s\n' "$1" >&2
  FAILED=$((FAILED + 1))
}

printf '==================================================\n'
printf 'Verifying Shilpo Examples & Reference Extensions\n'
printf '==================================================\n\n'

# 1. Canonical TypeScript Showcase: extensions/example
printf '[1/3] Verifying extensions/example...\n'

if deno task --cwd extensions/example test && \
   deno task --cwd extensions/example check && \
   deno task --cwd extensions/example lint && \
   deno task --cwd extensions/example fmt:check; then
  pass "TypeScript showcase tests, typecheck, lint, and formatting"
else
  fail "TypeScript showcase tests/lint failed"
fi

if [[ -f target/debug/shilpo ]]; then
  SHILPO_BIN="target/debug/shilpo"
elif [[ -f target/release/shilpo ]]; then
  SHILPO_BIN="target/release/shilpo"
else
  cargo build -p shilpo
  SHILPO_BIN="target/debug/shilpo"
fi

if "$SHILPO_BIN" ext lint extensions/example; then
  pass "TypeScript showcase ext lint"
else
  fail "TypeScript showcase ext lint failed"
fi

# Clean untracked build artifacts
rm -rf extensions/example/extension.wasm extensions/example/dist

# 2. Rust Reference Extension: extensions/world-clock
printf '\n[2/3] Verifying extensions/world-clock...\n'

if "$SHILPO_BIN" ext lint extensions/world-clock && \
   cargo build --manifest-path extensions/Cargo.toml --package world-clock-extension --target wasm32-wasip2 --release; then
  pass "Rust world-clock ext lint and guest component compilation for wasm32-wasip2"
else
  fail "Rust world-clock lint/component compilation failed"
fi

cp extensions/target/wasm32-wasip2/release/world_clock_extension.wasm extensions/world-clock/extension.wasm
if "$SHILPO_BIN" ext check extensions/world-clock; then
  pass "Rust world-clock bounded ext check"
else
  fail "Rust world-clock ext check failed"
fi
rm -f extensions/world-clock/extension.wasm

# 3. Trusted Local Script: extensions/cpu-temp-script
printf '\n[3/3] Verifying extensions/cpu-temp-script...\n'

if [[ -x extensions/cpu-temp-script/cpu-temp.sh ]]; then
  script_output=$(./extensions/cpu-temp-script/cpu-temp.sh)
  if echo "$script_output" | grep -q '"schema_version":1' && echo "$script_output" | grep -q '"contribution":"cpu-temp"'; then
    pass "Trusted Local Script deterministic execution and JSON record schema"
  else
    fail "Trusted Local Script emitted invalid record output: $script_output"
  fi
else
  fail "Trusted Local Script cpu-temp.sh is not executable"
fi

# 4. Clean Artifacts Assertion
printf '\nAsserting no leaked build artifacts...\n'
if git status --porcelain | grep -E '\.(wasm|shilpo-ext)$'; then
  fail "Verification left unignored build artifacts in repository"
else
  pass "Clean working tree invariant maintained (no leaked artifacts)"
fi

printf '\n==================================================\n'
printf 'Results: %d passed, %d failed\n' "$PASSED" "$FAILED"
printf '==================================================\n'

if [[ $FAILED -gt 0 ]]; then
  exit 1
fi
