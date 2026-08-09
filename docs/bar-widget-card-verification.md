# Bar widget card verification record

Verification date: 2026-08-09  
Delivery issues: [#58](https://github.com/sayeed205/shilpo/issues/58) and
[#59](https://github.com/sayeed205/shilpo/issues/59)

## Automated coverage

The deterministic card suites cover:

| Area | Evidence |
|---|---|
| Placement | Top, bottom, left, and right edges; safe corner clamping; non-zero monitor origins; compact/expanded dimensions; narrow-monitor suppression; persistent/preview collision avoidance; edge-band geometry |
| Coordinator | 350 ms hover intent, cancellation, grouped retargeting, pointer bridge, persistent replacement, same-source suppression, channel coexistence, every semantic dismissal, source/display/provider disappearance, stale generations, focus capture/restore, and auto-hide holds |
| Battery | Complete and partial values, unavailable metrics, charge-threshold semantics, aggregate/physical presentation, multiple-device selection, and provider capabilities |
| Workspace | Empty, one, five, overflow, active and urgent retention, spatial ordering, missing metadata/icons, connection states, source identity, vertical geometry, and Overview grouping |
| Runtime | Persistent and preview lifecycle projections, provider capability rejection, delayed preview opening, Overview dismissal, and shutdown |

Commands used:

```bash
cargo test -p shilpo-shell bar::cards::model::tests --lib
cargo test -p shilpo-shell bar::cards::adapter::tests --lib
cargo test -p shilpo-shell bar::widgets::workspaces::tests --lib
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p shilpo-shell
```

Card-specific results were green: 44 model tests, 10 adapter tests, and 6
workspace-widget tests. Workspace Clippy reported zero code warnings.

The full-workspace Nextest run initially exposed a device-client connection race:
`connect_on` returned before state-signal listeners were ready, so another client
could miss a command issued immediately after connection. The connection seam now
confirms listener readiness before loading its initial projection and returning.
The regression path passed 20 consecutive stress runs, the device-client suite
passed, and full workspace Nextest completed with 981 passed and 3 skipped tests.
The finding is tracked by [#68](https://github.com/sayeed205/shilpo/issues/68).

## Live verification performed

Environment: Niri, one 1920×1080 logical display at scale 1.0, top Hug bar,
reduced motion disabled.

- Battery source visibility, persistent open/close/toggle, outside dismissal,
  slide motion, focus behavior, live percentage/state/rate/health updates, compact
  single-battery details, charge thresholds, and light/dark presentation were
  exercised during development.
- Workspace initial hover intent, overview-style wallpaper/icon composition,
  rapid sibling traversal, immediate spatial retargeting, whole-group lifetime,
  click-to-focus, right-click Overview, and source state layers were exercised.
- Notification surfaces remained independently usable while card work was active.
- The shell was repeatedly rebuilt, installed, restarted, and confirmed active.

### Coverage matrix

| Verification path | Manual result | Automated or follow-up result |
|---|---|---|
| Top bar, M3 source states, motion, light/dark | Passed | Reducer and placement coverage also passed |
| Hover intent, rapid traversal, pointer bridge | Passed for Workspace | Timing, cancellation, grouped retargeting, and bridge coverage passed |
| Source toggle and outside-click dismissal | Passed for Battery | Every semantic dismissal path passed reducer coverage |
| Workspace click and right-click Overview | Passed | Overview dismissal/grouping coverage passed |
| Notifications while cards are active | Passed | Persistent/preview channel coexistence passed |
| Escape, focus loss, replacement, focus restoration | Not separately recorded live | Deterministic coordinator coverage passed; manual run tracked by #70 |
| OSD and critical-prompt coexistence | Not available in the recorded session | Cross-surface reducer behavior passed; manual run tracked by #70 |
| Auto-hide holds | Not available with the configured Hug bar | Hold acquisition/release coverage passed; manual run tracked by #70 |
| Source disappearance | Not forced live | Source/provider/display disappearance coverage passed |
| Degraded and reconnecting providers | Not forced live | Provider rejection and connection-state coverage passed |
| Reduced motion | Not enabled live | Reduced-motion state/placement behavior passed |
| Bottom/left/right bars, corners, narrow work areas | Not configured live | Deterministic geometry coverage passed |
| Non-1.0 scaling and multiple monitors | Hardware/configuration unavailable | Geometry and display-identity coverage passed; manual run tracked by #70 |
| Running Apps | Confirmed unchanged | Capture architecture remains deferred to #67 |

This machine did not provide additional physical outputs or non-1.0 scaling.
Those cases are covered deterministically at the geometry/state seams rather than
claimed as live observations. The remaining manual visual and cross-surface run
is an explicit blocker in [#70](https://github.com/sayeed205/shilpo/issues/70).

## Findings and blockers

- Workspace hover anchors initially failed because layer-shell display lookup and
  nested prepaint behavior were unreliable. The final implementation uses the
  authoritative bar display identity and exact live layout bounds.
- Rapid workspace traversal initially cancelled adjacent hover intent. Grouped
  source lifecycle and preview retargeting now keep one surface open and animate
  it between sources.
- The device-client listener-readiness race was fixed and stress-tested under #68.
- Repeated `gpui::window: window not found` journal errors were observed without a
  crash. Attribution and idempotent cleanup are tracked by
  [#69](https://github.com/sayeed205/shilpo/issues/69).

The stale-window diagnostic remains an explicit blocker rather than a silently
waived result. It did not crash the shell or invalidate the verified card
behavior, but its source must be attributed before the journal can be considered
clean under repeated surface churn. Consequently, #50 and #59 remain open until
#69 and the remaining manual verification in #70 are resolved. Issue #58 may be
closed because every uncompleted observation is now recorded as an explicit
blocker, as its acceptance criteria require.

## Deferred Running Apps previews

Running Apps remains unchanged. Pixel previews require a separately accepted
capture architecture covering privacy, protected content, memory lifetime,
cleanup, refresh policy, backpressure, and degraded/denied states. That future
design effort is tracked by
[#67](https://github.com/sayeed205/shilpo/issues/67); one-shot screenshot files are
not an acceptable streaming implementation.
