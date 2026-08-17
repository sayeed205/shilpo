# Theme

Pure Material 3 color computation and theming data types. Zero I/O, zero system dependencies (ADR-0002): it takes a seed
and produces palettes, and exposes the state transition seam the theme daemon drives.

## Language

**Seed**:
The ARGB color value that drives palette generation. Core never knows where a seed came from — it may be a user's custom
choice or the color extracted from a wallpaper. It is materialized into `source_argb` only when the active
`color_source` consumes that kind of seed. _Avoid_: wallpaper color, custom color, accent

**Source** (`color_source`):
Which seed feeds the palette: `Wallpaper` (external seed pushed by the daemon)
or `Custom` (a remembered user choice). A data variant in core; the daemon owns how a source produces its seed. _Avoid_:
theme source, seed source

**Selected mode** (`selected_mode`):
The mode the user picked: `system`, `light`, or `dark`. Intent, not outcome. _Avoid_: mode, user mode

**Resolved mode** (`resolved_mode`):
The mode actually applied to palettes and adapters. In `system` mode it follows the desktop portal's appearance; in
`light`/`dark` it mirrors the selection. _Avoid_: effective mode, current mode

**Revision**:
A monotonic version counter on the theme state. It increments whenever the effective state changes (never for no-ops),
so consumers can reject stale updates and order transitions. It is an ordering guard, not a count of user actions.
_Avoid_: version, change count

**Change kind**:
The daemon-emitted description of which aspects of the state changed (mode, source, variant, palette, wallpaper) for one
transition, so consumers can react to specific kinds of change — e.g. animate a mode toggle without re-deriving the
palette. _Avoid_: diff, effects, event

**Scheme variant** (`scheme_variant`):
The M3 style applied to a seed when generating a palette. `Auto` resolves to a concrete variant — from the seed's chroma
in core, or from an image-aware decision the producer injects when it materializes an external seed (the stored
selection stays `Auto` so the next seed re-derives). Any explicit variant pins the style for all future seeds. _Avoid_:
schema variant, algorithm, style

**Resolved variant** (`resolved_variant`):
The concrete `scheme_variant` actually applied to the current seed's palettes. When the selection is `Auto`, core
resolves it from the seed's chroma, or the producer's injected resolution wins when one is supplied at materialization.
An explicit selection is its own resolution. _Avoid_: effective variant, active variant, actual variant

**OKLCH Interpolation** (`interpolate_argb_oklch`):
Pure sRGB ↔ Oklab ↔ OKLCH ARGB color space interpolation primitive for perceptual theme transitions, shortest hue arc
calculation, and achromatic color handling (see [ADR-0014](../../docs/adr/0014-animated-theme-transitions.md)).
