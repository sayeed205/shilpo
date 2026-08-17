# Integrating a built-in bar widget card

Bar widget cards are internal Linux-shell surfaces. A widget supplies identity, capabilities, live source geometry,
preferred dimensions, and lazy content; the card coordinator owns timing, placement, focus, dismissal, collision
handling, animation, and layer-shell window lifetime.

Read [ADR-0004](adr/0004-bar-widget-card-surfaces.md) and the
[accepted design record](archive/bar-widget-cards-grill.md) before adding a provider.

## Provider and source identity

Implement `CardProvider` under `desktop/shilpo/src/shell/bar/cards/` and register one provider instance in
`ShellSurfaces::new`. Centralize the typed owner and source constructors in the provider module; other modules should
not repeat primitive owner strings.

A `CardSourceId` contains:

- the provider owner;
- a rendered widget instance, including the display identity when the widget can appear on multiple bars;
- an optional content key, such as a workspace or device ID.

Use a singleton source only when the widget truly has one shell-wide rendered source. Provider capabilities declare
hover and click independently. Unsupported requests are rejected before reaching the reducer.

## Geometry and sizing

Publish actual global element bounds through `AnchorUpdate` after layout. Keep the anchor live while the source is
mounted; open cards reposition when it changes. Send `AnchorRemoved` when authoritative data says the source no longer
exists.

`preferred_size` reports the dimensions needed by the current source and channel. The placement module clamps those
dimensions to the monitor work area, avoids the other card channel, and suppresses unusably small results. Do not create
or position layer-shell windows in a provider.

## Interaction patterns

For a click card, dispatch `PersistentToggle` or `PersistentToggleAt`. The latter is a fallback when exact prepaint
geometry is unavailable. Persistent cards may receive focus and are dismissed by source toggle, Escape, focus loss,
Overview, display removal, or shutdown.

For a passive preview, dispatch `SourceEnter` and `SourceLeave`. A related set of sources may additionally use
`SourceGroupEnter` and `SourceGroupLeave`: the first hover observes intent, sibling traversal retargets the visible
preview, and only leaving both the group and preview starts dismissal. Preview content must remain non-interactive.

Widgets query `CardCoordinator::source_state` to render idle, hover-pending, preview-open, and persistent-open states
with theme tokens. Preserve existing direct actions such as workspace focus and right-click Overview.

## Content and data lifetime

Render from authoritative runtime snapshots and reuse subscriptions already owned by the shell. Cache reusable expensive
inputs such as prepared wallpaper and resolved application icons; rebuild lightweight composition from current data.
Never mutate coordinator state from a provider render callback. Reconcile missing sources before rendering through
`source_available` and `refresh_owner`.

Provide useful empty, reconnecting, unavailable, and partial-data states. Keep domain actions and errors in the owning
widget/provider; use the shared coordinator only for card-surface policy.

## Verification checklist

- Add pure reducer tests for timing, cancellation, replacement, group traversal, dismissal, source disappearance, and
  visibility holds introduced by the widget.
- Add placement tests if the widget introduces a new size or collision case.
- Add provider/domain tests for complete, partial, empty, and degraded data.
- Verify all four bar edges, monitor offsets, constrained work areas, reduced motion, light/dark themes, and
  multi-monitor identity.
- Verify coexistence with the other channel and interactions with Overview, notifications, OSD, auto-hide, and existing
  widget actions.
- Run workspace tests and Clippy with zero code warnings.

Storybook is not used for shell-internal widgets. Add a Storybook story only when the work introduces or modifies a
genuinely reusable `shilpo-ui` primitive.

