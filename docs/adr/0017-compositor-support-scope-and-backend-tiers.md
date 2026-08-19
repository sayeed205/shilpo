# ADR-0017: Compositor Support Scope, Abstraction Boundary, and Backend Tiers

- **Status**: Accepted

## Context

Shilpo is a Linux desktop environment shell designed to operate across multiple Wayland compositors. Initial implementations hard-wired `NiriCompositorService` directly in the shell service hub, relied on an unsafe `CompositorCapabilities::default()` where all capabilities defaulted to `true` even during disconnected states, included Niri-specific scrollable layout fields (`column`, `row`) in the neutral `WindowInfo` struct, and bypassed the compositor abstraction in the settings application using direct `niri-ipc` socket calls.

Furthermore, issue #106 (Hyprland backend) initially included "Shortcut registration" in its scope, which conflicts with ADR-0015's established architecture of opt-in, out-of-band generated configuration includes.

We required an architectural contract that:
1. Defines a sustainable multi-compositor strategy maximizing compositor compatibility without multiplying per-compositor maintenance overhead.
2. Preserves ADR-0006's single, atomically revisioned domain port contract and capability-gated command broker.
3. Expresses backend capabilities accurately, degrading closed by default when disconnected or unsupported.
4. Separates compositor-agnostic metadata from backend-specific extras.
5. Provides reliable, testable compositor detection and dynamic backend instantiation.
6. Maintains the boundary between runtime IPC commands and out-of-band shortcut projection.

---

## Decision

### 1. Two-Tier Backend Architecture

We adopt a two-tier backend model where broad compositor coverage is achieved via standardized Wayland protocols rather than bespoke IPC implementations for every compositor:

- **Tier 0 — `NullCompositorBackend`**: Fallback backend used when no compositor is recognized or when candidate backends fail to initialize. Reports `DomainLifecycle::Unavailable`, `WindowIdentity::None`, and all capabilities `false`. The shell boots fully; non-compositor services (audio, brightness, notifications, tray, theme) operate normally, and capability gating cleanly disables window and workspace management UI.
- **Tier 1 — Generic Wayland Protocol Backend**: A unified backend operating over standardized protocols: `ext-workspace-v1` for workspace management, and `ext-foreign-toplevel-list-v1` / `wlr-foreign-toplevel-management-v1` for window tracking and activation. This single backend covers labwc, dwl, river, KDE Plasma, Sway, and future wlroots-based compositors without compositor-specific code. Reports `WindowIdentity::Fuzzy` (protocol handles support focus and close, but IDs are not persistent across reconnects).
- **Tier 2 — Named IPC Backends**: Dedicated backends using proprietary compositor IPC sockets for compositors requiring deep integration, custom layout metadata, or exact window identities (`WindowIdentity::Exact`). Includes `NiriCompositorService` and Hyprland (#106).

### 2. Single Domain Trait and Revisioned Snapshot Atomicity

We ratify the existing `CompositorAdapter` trait and single `CompositorSnapshot`. We explicitly reject splitting the adapter into per-capability fine-grained traits (`WorkspacePort`, `WindowPort`, `KeyboardLayoutPort`).

**Rationale**: ADR-0006 establishes that each service domain maintains one authoritative owner and publishes one atomically revisioned snapshot (`DomainVersion { owner_generation, revision }`). Slicing the domain port into multiple independent traits would require independent revision tracking per sub-port, breaking the atomic snapshot invariants validated by the conformance test harness. Unsupported operations are represented cleanly via the declarative capability matrix and rejected with `RejectionReason::Unsupported` at the command broker boundary.

`CompositorAdapter` is scoped to IPC observation and typed commands. Image- or pixel-producing methods (workspace/window thumbnails) are deliberately **excluded** and belong to the capture domain under ADR-0003 — they carry frame ownership, memory-budget, and invalidation concerns the snapshot model does not express (see [#134](https://github.com/sayeed205/shilpo/issues/134), [#67](https://github.com/sayeed205/shilpo/issues/67)).

### 3. Backend-Derived Capability Matrix and Window Identity Degradation

`CompositorCapabilities::default()` is inverted to **all-false** (`can_create_workspace: false`, `can_move_window: false`, `can_focus_window: false`, `can_focus_workspace: false`, `can_close_window: false`). The system degrades closed: a backend that does not explicitly report capability cannot execute commands.

We add `WindowIdentity` to `CompositorCapabilities`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowIdentity {
    /// No window model available.
    None,
    /// Protocol handles only; focus/close work, but IDs are ephemeral across reconnects.
    Fuzzy,
    /// Stable, addressable compositor-assigned window IDs.
    Exact,
}
```

Backends must derive their reported capabilities from their live connection state. For example, `NiriCompositorService` reports `can_* = true` and `WindowIdentity::Exact` **only** when `DomainLifecycle::Ready`; during `Connecting`, `Reconnecting`, `Unavailable`, or degraded states, all capabilities revert to `false` and `WindowIdentity::None`.

### 4. Neutral Data Structures and Typed Backend Extras

Compositor-neutral data structures contain only universally applicable window and workspace metadata:
- `WindowInfo` contains spatial layout coordinates (`layout_x`, `layout_y`), geometry, focus, and urgency. Niri-specific scrollable tiling fields (`column`, `row`) are removed from `WindowInfo`.
- `WorkspaceInfo.idx` is widened from `u8` to `u32` to accommodate compositors with arbitrary workspace numbering.
- Backend-specific metadata is moved to a typed enum on `CompositorSnapshot`:

```rust
#[derive(Clone, Debug, PartialEq, Default)]
pub enum CompositorExtras {
    #[default]
    None,
    Niri(NiriExtras),
}

#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct NiriExtras {
    pub window_positions: std::collections::HashMap<u64, (usize, usize)>,
}
```

### 5. Deterministic, Injectable Compositor Detection

We add `desktop/services/src/compositor/detect.rs` providing `CompositorKind` and detection routines:
- Reliable compositor-set environment variables are evaluated first in strict priority order:
  1. `NIRI_SOCKET` (or `NIRI_SOCKET_PATH`) -> `CompositorKind::Niri`
  2. `HYPRLAND_INSTANCE_SIGNATURE` -> `CompositorKind::Hyprland`
  3. `SWAYSOCK` -> `CompositorKind::Sway`
  4. `LABWC_PID` -> `CompositorKind::Labwc`
- If no compositor-specific variable is present, fallback checks evaluate case-insensitive substring matching across `XDG_CURRENT_DESKTOP`, `XDG_SESSION_DESKTOP`, and `DESKTOP_SESSION` for `niri`, `hyprland`, `sway`, `labwc`, `dwl`, `river`, `kde`/`plasma`.
- Detection is parameterized via an environment query function (`detect_from(&dyn Fn(&str) -> Option<String>)`), ensuring hermetic, deterministic tests without modifying process environment or relying on global `OnceLock` caching (conforming to ADR-0006 invariant 8).

### 6. Backend Selection Registry

We replace the hard-wired `NiriCompositorService::new()` instantiation in `service_hub.rs` with a backend registry. The registry maps `CompositorKind` to an ordered candidate chain, attempts instantiation in priority order, and falls back to `NullCompositorBackend`.

The detected compositor kind and selected backend name are logged at `info` level during startup.

### 7. Keybinding Projection Boundary (ADR-0015 Alignment)

Shortcut registration is explicitly **excluded** from `CompositorAdapter`. In accordance with ADR-0015, shortcut management is an out-of-band projection into compositor configuration files (e.g. `$XDG_CONFIG_HOME/shilpo/generated/niri-keybindings.kdl` for Niri; future configuration includes for Hyprland). `CompositorAdapter` remains strictly focused on runtime IPC observation and commands, without acquiring filesystem write side-effects.

Direct `niri_ipc` socket access in `settings` is eliminated, and `niri-ipc` is removed as a dependency of `shilpo`. Window activation from the settings application routes strictly through `CompositorAdapter` / `CompositorCommand::FocusWindow`.

---

## Consequences

### Positive

- **Extensibility**: Shilpo can run on any Wayland compositor with a graceful degraded fallback (`NullCompositorBackend`) and broad future support via the Tier 1 generic protocol backend.
- **Safety & Robustness**: Inverting capability defaults prevents sending commands to disconnected or unsupported backends.
- **Clean Separation of Concerns**: IPC adapters remain pure communication layers; configuration file generation remains separate under ADR-0015.
- **Isolation**: External crates like `niri-ipc` are encapsulated entirely within `shilpo-services::compositor::niri`.

### Negative & Follow-ups

- Tier 1 generic protocol backend (`ext-workspace-v1` / `ext-foreign-toplevel-list-v1`) is tracked in [#107](https://github.com/sayeed205/shilpo/issues/107).
- Hyprland backend ([#106](https://github.com/sayeed205/shilpo/issues/106)) is updated to target this registry and exclude shortcut registration per Decision 7.
