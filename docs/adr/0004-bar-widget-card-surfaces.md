# ADR-0004: Bar Widget Card Surfaces

- **Status**: Accepted

Bar widgets expose supplementary content through cards rendered on dedicated layer-shell surfaces rather than inside the
bar window. This ADR records the structural decisions that span all widgets; widget-specific content and behavior details
live in the [design record](../archive/bar-widget-cards-grill.md). Parent delivery
tracker: [#50](https://github.com/sayeed205/shilpo/issues/50).

## Dedicated per-monitor surfaces

Each active monitor may lazily create up to two reusable card surfaces — one for the persistent-click channel and one for
the temporary-hover channel. Surfaces are Wayland layer-shell windows managed by the shell (`desktop/shilpo`). Hidden
surfaces clear
their content; surfaces are destroyed when their monitor disappears or the shell configuration is rebuilt.

**Rejected alternative: rendering cards inside the bar window.** The bar is a narrow, auto-sized layer-shell surface.
Expanding it to contain arbitrary card content would fight the compositor's layer-shell sizing model, require the bar to
manage focus semantics it currently avoids, and couple card layout to bar geometry.

## Two-channel coordinator in `desktop/shilpo`

A central coordinator owns a shell-wide persistent-click channel (zero or one card) and a temporary-hover channel (zero
or one card). Each open entry carries owner identity, monitor, anchor geometry, and lifecycle state. Channel limits are
global, not per-monitor: at most one persistent card and one preview may be visible across the entire shell.

The coordinator is responsible for placement, timing, collision avoidance, focus, dismissal, auto-hide holds, and surface
lifecycle. Built-in widgets supply a stable source identity, independently declared hover and click capabilities, live
anchor geometry, an internal content provider, and preferred dimensions for each source/channel. The coordinator clamps
those dimensions to the available monitor space. The built-in provider contract remains internal to the Shell.

**Rejected alternative: per-widget surface management.** Letting each widget create and manage its own surface would
duplicate placement logic, make shell-wide exclusivity unenforceable, and scatter focus and dismissal policies across
unrelated widget implementations.

**Rejected alternative: a single channel for both hover and click.** A single open/closed channel cannot represent the
coexistence of a persistent clicked card and a temporary hover preview from different widgets, which is a core
interaction requirement.

## Widget-owned content

Widgets own their domain content, actions, and domain-specific error handling. The coordinator has no knowledge of
battery data, workspace state, or media controls. Content factories are invoked lazily; data subscriptions already
required for the bar representation are reused rather than duplicated.

## Extension boundary

Extension bar menus use a constrained declarative projection into the persistent channel rather than the built-in
content-provider contract. A bar-menu contribution links to one bar-widget contribution and supplies a `ViewTree`; it
does not declare a width, height, or size tier. The Shell measures the tree's intrinsic content size under host-owned
constraints, adds host-owned card chrome, clamps the result to the monitor work area and safety limits, and enables
overflow scrolling only where the measured content exceeds those bounds. Placement uses the resulting bounded size.

This keeps content responsive to theme metrics, fonts, localization, and live updates without making surface geometry
part of the extension API. Extensions do not receive raw shell-surface access, GPUI types, placement controls, or focus
policy. The Shell maps typed menu lifecycle transitions to extension events while retaining ownership of exclusivity,
dismissal, focus restoration, and surface lifetime.

**Rejected alternative: extension-declared dimensions or size tiers.** Fixed tiers waste space for small content and
force discontinuous jumps for larger content. Arbitrary dimensions make extension authors predict host typography and
monitor constraints while expanding the public validation surface. Both encode presentation policy in the manifest
instead of measuring the declarative content the host already owns.

**Rejected alternative: unbounded intrinsic sizing.** A malformed or adversarial tree could request an unusable surface.
Dynamic sizing is therefore always constrained by host policy and available monitor geometry; those limits are not guest
configuration.

**Rejected alternative: exposing raw shell surfaces to extensions.** Direct surface access would bypass the coordinator's
exclusivity, placement, and focus rules, creating an unsandboxable escape hatch.

## Consequences

- At most two reusable card surfaces per active monitor (one per channel), created lazily and reclaimed on monitor loss.
- At most one persistent card and one preview visible shell-wide at any time.
- Persistent clicked surfaces accept focus; temporary hover previews remain non-interactive.
- Escape, source-widget toggle, outside click, and focus loss dismiss a persistent card. Dismissal restores focus to the
  previously focused application when that application remains available.
- All surface policy — layer-shell creation, placement, focus, collision, dismissal — remains Linux-specific inside
  `desktop/shilpo` (ADR-0001 principle).
- Existing `shilpo-ui` presentation primitives (`Card`, `Popover`, `HoverCard`) are reused for card content where
  applicable; genuinely missing generic primitives are added to `shilpo-ui` with interactive Storybook stories.
- The provider contract serves built-ins only. Extension bar menus use a separate declarative adapter with host-measured,
  host-bounded intrinsic sizing rather than raw surface control or direct access to the provider contract.
- Widget-specific Battery, Workspace, and Running Apps behavior is recorded in the
  [design record](../archive/bar-widget-cards-grill.md), not in this ADR.
