# Bar Widget Cards — Design Grill

Status: **accepted design baseline**. Architectural decisions are recorded in
[ADR-0006](adr/0006-bar-widget-card-surfaces.md). Parent delivery tracker:
[#50](https://github.com/sayeed205/shilpo/issues/50). This document records settled design decisions and widget-specific
behavior; it is not an implementation plan or authorization to change product code.

## Goal

Build a coherent card system for bar widgets that can support a deliberate mix of passive information and interactive controls. The architecture should make later built-in widget integrations straightforward and leave a clean path for a constrained extension API.

## Settled decisions

### Shared system

- Use one shared card system with widget-specific content.
- Use a dedicated card surface per monitor rather than fitting cards inside the small bar layer-shell window.
- A central coordinator owns the active card, trigger mode, source widget, anchor geometry, monitor, focus, and dismissal lifecycle.
- At most one persistent clicked card and one temporary hover preview may be visible together.
- Concurrent surfaces remain separate and never overlap. The persistent card keeps placement priority; the hover preview uses the next collision-free placement and may shrink within its tier.
- Clicking another card-enabled widget replaces the persistent card atomically.
- The internal built-in provider contract accepts a stable widget/card identity, independently declared hover/click capabilities, live anchor geometry and monitor, a content provider/controller, and preferred size tier.
- Widgets own their domain content and actions. The coordinator owns surfaces, two-channel state, placement, shell-wide exclusivity, focus, delays, collision, and dismissal.
- The two explicit channels are `persistent` (zero or one clicked card) and `preview` (zero or one hover card). Each open entry carries owner identity, monitor, anchor, and lifecycle state.
- Each active monitor may lazily create up to two reusable card surfaces, one per channel. Hidden surfaces clear content; surfaces are destroyed when their monitor disappears or shell configuration is rebuilt.

### Per-widget capabilities

- Hover and click are independent capabilities. A widget may support either, both, or neither.
- Hover is for passive previews. Hover cards contain no interactive controls.
- Click is for persistent interactive cards where the widget benefits from one.
- A clicked card receives focus. Escape or clicking the originating widget again closes it.
- Outside-click also dismisses a clicked card. On dismissal, focus returns to the previously focused application/window.
- The bar itself has no keyboard-navigation or keyboard-focus mode.
- Content complexity follows the widget: a clock/date card may be small, while media may expose several controls.
- All cards still use a common structural vocabulary. A card is one surface with sections and, when justified, at most one secondary detail level. Deeper workflows hand off to Settings or the owning application.

### Existing actions

- Preserve frequent direct actions such as focusing a workspace or application, controlling playback, and toggling Caffeine.
- Use only ordinary hover and click triggers for now. Avoid adding chevrons, click-and-hold, or a new universal secondary gesture.
- Existing gesture ownership remains relevant; for example, workspace right-click already opens the overview.
- Exact trigger behavior is decided separately for each widget rather than imposed globally.

### Interaction quality

- A clicked card remains open until explicitly dismissed and is not displaced by hovering another widget.
- Another widget's hover preview may appear temporarily alongside a clicked card.
- When a widget's clicked card is open, that same widget's hover preview is suppressed.
- Hover transitions initially target about 350 ms to open and 200 ms to close, with cancellation and a pointer bridge so moving from the widget into its preview is stable.
- An informational hover preview remains open while the pointer is over either its source widget or the preview, then observes the close delay after leaving both.
- Cards must handle top, bottom, left, and right bar placement; multiple monitors; narrow available space; and edge-aware placement.
- Cards open inward from the configured bar edge, initially align with their source, clamp to monitor bounds, and shift along the bar axis when required.
- Anchor geometry stays live. Open cards follow bar reflow and close if their source disappears. Repositioning snaps when reduced motion is enabled.
- Cards require reduced-motion behavior and useful loading, unavailable, empty, reconnecting, and error states where applicable.
- Data acquisition and expensive rendering begin only when required, unless a widget already needs the data for its bar representation.
- Data and view lifetimes are separate: reuse subscriptions already needed by the bar, create card views lazily, and release card-only resources after closure.
- Interactive controls use authoritative service state with a visible pending projection. Confirmation reconciles the projection; rejection or timeout restores authoritative state and reports failure.
- Card-local action failures appear inline near the affected control and preserve retry context. Routine success is reflected by authoritative state. Global continuous feedback uses the existing OSD. Durable or cross-surface failures use shell notification toasts; an important late failure is promoted if its card has already closed.
- Cards share surface styling, spacing, corner radius, elevation, motion, and optional section primitives without requiring a universal header or footer.
- Cards use compact, standard, and expanded sizing tiers constrained by the monitor work area; sparse content may shrink below a tier's maximum.
- Exclusivity is shell-wide rather than per-monitor. The persistent card and preview may occupy different monitors, but their channel limits remain global.
- Cards use no triangular pointer/caret. Proximity, motion origin, and the source widget's interaction state communicate ownership.
- Source widgets follow the text/icon/ghost-button state vocabulary: subtle hover, visible focus when applicable, and a stronger selected container state while persistent content is open. Persistent selection is more prominent than hover-preview selection.
- Opening uses M3 emphasized easing over roughly 200–250 ms with short inward translation plus fade/scale; previews may be slightly faster. Reduced motion uses a near-instant fade.
- Rapid traversal cancels pending previews immediately. An already-visible preview survives briefly during transit and is replaced only after the new target satisfies hover intent.
- Either open or pending channel holds the bar visible; auto-hide is released after both channels and their close delays finish.
- Persistent cards close when focus moves to an unrelated surface, while legitimate owned child/dialog focus is exempt.
- Opening Overview closes both channels and cancels pending previews.
- OSDs remain non-interactive above cards. Notification surfaces remain independent and should avoid card bounds when practical. Cards never obscure critical system prompts.
- Source styling reuses the same interaction-state tokens and rules as text/icon/ghost buttons without requiring every widget to be implemented as a `Button`.

### Scope and rollout

- Start with Battery, Workspaces, and Running Apps, then integrate other visible widgets incrementally.
- Battery uses click for a persistent status/details card.
- Workspaces use hover for previews of the applications/windows open on that workspace. Clicking retains its existing focus action and closes the preview.
- Running Apps hover is deferred until Shilpo has suitable window-capture functionality. Clicking retains its existing focus action.
- The supplied browser-tab screenshot is a visual reference for the eventual Running Apps preview treatment: application identity, window title, and prominent visual thumbnail. It does not request browser-tab integration.
- Audio and Settings are configured but currently do not render, and Bluetooth appears not to receive live updates. These are known follow-up concerns, not part of the initial card slice unless explicitly selected later.
- The initial coordinator serves built-ins. Its concepts should remain generic enough for a later declarative, capability-checked extension API; extensions do not receive raw shell-surface access.

## Repository facts discovered during the grill

Current rendered built-ins include Workspaces, Running Apps, Clock, Date, Media, Sysinfo, Network, Bluetooth, Caffeine, and Battery. Audio and Settings occur in configuration but currently fall through the bar renderer.

`shilpo-ui` already contains `Popover`, `HoverCard`, and `Card`. They provide useful content and lifecycle precedents, but the shell card system likely needs a separate surface because the bar window is small and currently uses no keyboard interactivity.

The existing Super+Space overview uses wallpaper-backed workspace cards with application-icon proxies; it does not capture window pixels. Its workspace composition and visual language are the starting reference for Workspace hover previews.

Current Battery data carried end-to-end is percentage, charging status, and presence. UPower can additionally provide semantic state, time estimates, energy and rate values, health/capacity, voltage, temperature, technology, warning level, identity, and sometimes cycle count. Availability varies by hardware, and aggregate display-device data may not contain physical-battery details.

The Battery domain should expose all meaningful, stable UPower battery fields through Shilpo's typed device protocol and DBus contract, preserving optionality. It should model an aggregate summary plus zero-or-more physical batteries. Card content remains curated separately from transport completeness.

The Battery card leads with percentage, semantic state, time to full/empty, charge/discharge rate, and health. A secondary details section contains available model, technology, energy, voltage, temperature, cycles, and other technical properties; unavailable properties are omitted. The aggregate system battery appears first. Physical power-supply batteries appear as compact rows whose selection opens the permitted secondary detail level. Peripheral batteries are not included in the system aggregate.

The Battery widget remains hidden when no system battery exists. If data disconnects while its card is open, the card stays open with a reconnecting or unavailable state.

Shilpo currently has screenshot-oriented output capture, not a live window-thumbnail pipeline. Niri can produce one-shot window screenshots, but repeated screenshot files are not an appropriate streaming design. Running Apps visual previews therefore remain a later capture-architecture milestone.

## First delivery slice

The first implementation slice, when authorized, consists of:

1. The shared coordinator and dedicated per-monitor surfaces for persistent-click and temporary-hover channels.
2. A Battery click card.
3. A Workspace hover preview similar to the existing overview's workspace representation.
4. Existing Running Apps behavior left unchanged.

The Workspace preview is a compact, presentation-neutral reuse or extraction of the overview's visual model rather than an embedding of the full overview. It updates from compositor events, represents the overview-like application/window composition, provides a clear empty state, prioritizes focused and urgent windows under crowding, and caps rendered entries.

Its visual core consists of the overview's prepared blurred/downscaled wallpaper or themed fallback, scrim and surface tint, equal-width window regions, centered application icons, active-workspace outline, rounding, and shadow. It excludes overview-only search, launcher, filmstrip, drag/drop, and window-selection behavior. Workspace identity, window counts, and window titles stay out of the visual miniature but remain available to accessibility labels. The preview is informational; pointer entry only keeps it open.

Crowded previews show up to five spatially ordered window regions and retain active and urgent windows before the spatially nearest candidates. Hidden-window counts remain presentation metadata rather than visible chrome. Wallpaper handling reuses overview preparation and does not introduce screen capture or a second wallpaper-processing path.

Workspace preview caching retains only reusable inputs such as prepared wallpaper and resolved icons. Its lightweight composition is rebuilt from the latest compositor snapshot on opening and updates while visible.

Prepared wallpaper becomes a shell-runtime-owned presentation resource keyed and invalidated by wallpaper identity. It prepares the blurred/downscaled image asynchronously and notifies both Overview and card consumers. The I/O-bearing resource does not belong in the pure `shilpo-theme` crate.

## Architecture boundaries

- The coordinator, placement engine, focus rules, and layer-shell surface lifecycle belong to `shilpo-shell`.
- Existing `shilpo-ui` card/popover visual primitives are reused. Core UI changes occur only for genuinely generic missing primitives and require an interactive Storybook story with fully wired events.
- Battery expansion is a typed vertical change from UPower adapter through versioned device protocol/DBus and client projection to shell presentation. Optional semantic fields replace raw UPower integers and loosely typed property maps.
- The coordinator state and placement calculations should be pure where practical. Structured debug logs cover channel transitions, owner identity, monitor, placement, dismissal, and surface lifecycle without recording window titles, battery serials, or other sensitive values.

## Delivery and issue structure

The work is tracked by parent issue [#50](https://github.com/sayeed205/shilpo/issues/50) and its linked child issues. GitHub sub-issues and native dependency edges encode this task order:

1. [#51](https://github.com/sayeed205/shilpo/issues/51) records the architecture in this ADR.
2. [#52](https://github.com/sayeed205/shilpo/issues/52) extracts and tests the shared wallpaper-preview resource.
3. [#53](https://github.com/sayeed205/shilpo/issues/53) extracts the presentation-neutral workspace miniature.
4. [#54](https://github.com/sayeed205/shilpo/issues/54) builds and tests the coordinator state machine and placement engine.
5. [#55](https://github.com/sayeed205/shilpo/issues/55) integrates Workspace hover.
6. [#56](https://github.com/sayeed205/shilpo/issues/56) expands the Battery domain through DBus/client.
7. [#57](https://github.com/sayeed205/shilpo/issues/57) builds and integrates the Battery card.
8. [#58](https://github.com/sayeed205/shilpo/issues/58) verifies cross-feature behavior and multi-monitor scenarios.
9. [#59](https://github.com/sayeed205/shilpo/issues/59) reconciles documentation and closes the delivery slice.

No permanent user-facing feature flag is planned. Any temporary internal gate used during development is removed before completion.

## Verification contract

Coordinator and placement tests cover hover timing/cancellation, two-channel coexistence, same-owner suppression, replacement, all dismissal paths, disappearing anchors, every bar edge/corner, collision avoidance, monitor transitions, reduced motion, and auto-hide holds. Shell integration tests cover surface lifecycle.

Battery tests cover complete, partial, absent, reconnecting, and multiple-device data. Workspace tests cover empty, single-window, five-window, overflow, active-window viewport, urgency, missing icons, wallpaper pending/failure, and compositor reconnect cases.

The first slice is complete only when:

- Battery and Workspace contracts in this document are delivered, while Running Apps behavior remains unchanged.
- Edge placement, multi-monitor behavior, hover intent, pointer bridge, focus, dismissal, coexistence, and auto-hide behavior pass verification.
- Missing and degraded data states remain usable.
- Overview behavior remains intact after extraction.
- Relevant tests pass and Clippy reports zero warnings.
- The ADR and design record match delivered behavior.
- Manual visual checks cover M3 interaction states, motion, scaling, and light/dark themes.

## ADR scope

The ADR records dedicated per-monitor surfaces, the two-channel coordinator, ownership boundaries, rejection of in-bar rendering, and architectural consequences. Widget-specific content remains in this design record.

## Open decisions

- When Running Apps capture is revisited, grill privacy, protected-content, memory lifetime, and refresh policies before enabling pixel previews.
