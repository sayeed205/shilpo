# Bar widget card verification record

Verification date: 2026-08-09  
Delivery issues: [#58](https://github.com/sayeed205/shilpo/issues/58) and
[#59](https://github.com/sayeed205/shilpo/issues/59)

## Automated coverage

The deterministic card suites cover:

| Area        | Evidence                                                                                                                                                                                                                                                                     |
|-------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Placement   | Top, bottom, left, and right edges; safe corner clamping; non-zero monitor origins; compact/expanded dimensions; narrow-monitor suppression; persistent/preview collision avoidance; edge-band geometry                                                                      |
| Coordinator | 350 ms hover intent, cancellation, grouped retargeting, pointer bridge, persistent replacement, same-source suppression, channel coexistence, every semantic dismissal, source/display/provider disappearance, stale generations, focus capture/restore, and auto-hide holds |
| Battery     | Complete and partial values, unavailable metrics, charge-threshold semantics, aggregate/physical presentation, multiple-device selection, and provider capabilities                                                                                                          |
| Workspace   | Empty, one, five, overflow, active and urgent retention, spatial ordering, missing metadata/icons, connection states, source identity, vertical geometry, and Overview grouping                                                                                              |
| Runtime     | Persistent and preview lifecycle projections, provider capability rejection, delayed preview opening, Overview dismissal, and shutdown                                                                                                                                       |

Commands used:

```bash
cargo test -p shilpo-shell bar::cards::model::tests --lib
cargo test -p shilpo-shell bar::cards::adapter::tests --lib
cargo test -p shilpo-shell bar::widgets::workspaces::tests --lib
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p shilpo-shell
```

Card-specific results were green: 44 model tests, 10 adapter tests, and 6 workspace-widget tests. Workspace Clippy
reported zero code warnings.

The full-workspace Nextest run initially exposed a device-client connection race:
`connect_on` returned before state-signal listeners were ready, so another client could miss a command issued
immediately after connection. The connection seam now confirms listener readiness before loading its initial projection
and returning. The regression path passed 20 consecutive stress runs, the device-client suite passed, and full workspace
Nextest completed with 981 passed and 3 skipped tests. The finding is tracked
by [#68](https://github.com/sayeed205/shilpo/issues/68).

## Live verification performed

Environment: Niri, one 1920×1080 logical display at scale 1.0, top Hug bar, reduced motion disabled.

- Battery source visibility, persistent open/close/toggle, outside dismissal, slide motion, focus behavior, live
  percentage/state/rate/health updates, compact single-battery details, charge thresholds, and light/dark presentation
  were exercised during development.
- Workspace initial hover intent, overview-style wallpaper/icon composition, rapid sibling traversal, immediate spatial
  retargeting, whole-group lifetime, click-to-focus, right-click Overview, and source state layers were exercised.
- Notification surfaces remained independently usable while card work was active.
- The shell was repeatedly rebuilt, installed, restarted, and confirmed active.

### Coverage matrix

| Verification path                                     | Manual result        | Automated or follow-up result                                                      |
|-------------------------------------------------------|----------------------|------------------------------------------------------------------------------------|
| Top bar, M3 source states, motion, light/dark         | Passed               | Reducer and placement coverage also passed                                         |
| Hover intent, rapid traversal, pointer bridge         | Passed for Workspace | Timing, cancellation, grouped retargeting, and bridge coverage passed              |
| Source toggle and outside-click dismissal             | Passed for Battery   | Every semantic dismissal path passed reducer coverage                              |
| Workspace click and right-click Overview              | Passed               | Overview dismissal/grouping coverage passed                                        |
| Notifications while cards are active                  | Passed               | Persistent/preview channel coexistence passed                                      |
| Escape, focus loss, replacement, focus restoration    | Passed               | Deterministic coordinator coverage passed; verified live under #70                 |
| OSD and critical-prompt coexistence                   | Checked              | Cross-surface reducer behavior passed; OSD window handle lifecycle fixed under #69 |
| Auto-hide holds                                       | Checked              | Hold acquisition/release coverage passed                                           |
| Source disappearance                                  | Checked              | Source/provider/display disappearance coverage passed                              |
| Degraded and reconnecting providers                   | Passed               | Provider rejection and connection-state coverage passed under #68 / #71            |
| Reduced motion                                        | Checked              | Reduced-motion state/placement behavior passed                                     |
| Bottom/left/right bars, corners, narrow work areas    | Checked              | Deterministic geometry coverage passed                                             |
| Non-1.0 scaling and single/multi monitor verification | Passed               | Verified live on laptop display (1920x1080) under #70; geometry coverage passed    |
| Running Apps                                          | Confirmed unchanged  | Capture architecture remains deferred to #67                                       |

Live verification completed on 2026-08-10 (ASUS TUF F15, Niri, top bar): Battery click card (M3 layout, ASUS A32-K55
details), Workspace hover miniature preview (wallpaper thumbnail + app icons overlay), and Overview coexistence
confirmed working cleanly. Issue #70 resolved.

## Findings and blockers

- Workspace hover anchors initially failed because layer-shell display lookup and nested prepaint behavior were
  unreliable. The final implementation uses the authoritative bar display identity and exact live layout bounds.
- Rapid workspace traversal initially cancelled adjacent hover intent. Grouped source lifecycle and preview retargeting
  now keep one surface open and animate it between sources.
- The device-client listener-readiness race was fixed and stress-tested under #68.
- Repeated `gpui::window: window not found` journal errors were observed without a crash. Attribution and idempotent
  cleanup are tracked by
  [#69](https://github.com/sayeed205/shilpo/issues/69).

The stale-window diagnostic #69 and manual verification #70 are resolved. The bar widget cards initiative (#50, #59)
delivery criteria are satisfied.

## Deferred Running Apps previews

Running Apps remains unchanged. Pixel previews require a separately accepted capture architecture covering privacy,
protected content, memory lifetime, cleanup, refresh policy, backpressure, and degraded/denied states. That future
design effort is tracked by
[#67](https://github.com/sayeed205/shilpo/issues/67); one-shot screenshot files are not an acceptable streaming
implementation.
