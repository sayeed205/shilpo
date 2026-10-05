# Filled button implementation notes

## Source snapshot

- AndroidX source tree: `11ece46a49d485c7644e53cb0684a611d7a0ec10`.
- `compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Button.kt`
  (last touched by `0237eaa953c0e83848b8ecc2d42f67d5fa3eac1d`). The expressive `Button` overload
  reads `PressInteraction` state, selects the pressed or resting `ButtonShapes`, and uses
  `MotionSchemeKeyTokens.DefaultEffects` for its morph.
- `tokens/ButtonSmallTokens.kt` (`v0_11_0`): 40 dp container height, full-round resting shape, small
  pressed shape, and 16 dp horizontal content space.
- `tokens/ShapeTokens.kt` (`14_1_0`): `CornerFull` and `CornerSmall` (8 dp).
- `tokens/FilledButtonTokens.kt` (`v0_11_0`): primary/on-primary colors; disabled container at 10%
  on-surface and disabled label at 38% on-surface-variant.
- `tokens/ExpressiveMotionTokens.kt` (`v0_14_0`): DefaultEffects damping ratio 1.0 and stiffness
  1600. The button intentionally uses this non-bouncy effects spring for its shape morph.
- `compose/material3/material3-ripple/src/commonMain/kotlin/androidx/compose/material3/ripple/RippleAnimation.kt`
  (same source snapshot), lines 49-54 and 83-114/157-182: bounded origin/radius, 75 ms fade-in,
  225 ms expansion, 150 ms fade-out, 30%-of-largest-dimension start radius, and diagonal/2 + 10 dp
  ending radius.
- `compose/material3/material3-ripple/src/commonMain/kotlin/androidx/compose/material3/ripple/CommonRipple.kt`,
  lines 33-54: press starts a ripple; release/cancel finish it; a newer press finishes existing
  ripples before adding another.
- `compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens/StateTokens.kt`
  and `.../material3/Ripple.kt`: pressed opacity is 0.10 and default ripple color is
  `LocalContentColor`.
- Amane library source: `30800248fc51a663ec1f852047818641d6218786`.

## What this module carries over

`components::button::filled` keeps the caller interface small: a stable, globally unique logical
name (not a label or ordering), theme, label, enabled flag, and activation callback. It owns
per-button contact state and the analytic critical spring. Interaction entries currently persist
for the process lifetime. The resting/pressed radius pair, 40 px height, 16 px horizontal padding,
14 px medium label, filled palette mapping, disabled alpha, and stiffness/damping values follow the
tokens above. The spring uses the exact critically damped solution and carries its current velocity
when the pointer reverses the morph.

The bounded press ripple is drawn with Amane's `Canvas`/`Circle` primitives. It starts at the
pointer-down position with a radius of 30% of the larger button dimension, moves its center linearly
to the button center, and grows to half the button diagonal plus 10 logical pixels. Radius growth
uses Compose `FastOutSlowInEasing` over 225 ms; opacity rises linearly over 75 ms. On release or the
available cancel signal, the ripple remains fully visible until expansion completes and then fades
linearly over 150 ms. It uses `theme.on_accent` at the Material pressed opacity of 10%. A new press
finishes any older ripples before starting its own. The canvas is clipped by the outer rectangle, so
the clip follows the same animated corner radius as the button. It is drawn over the label, matching
AndroidX's content-then-ripple draw order (`material3-ripple/.../Ripple.kt`). Amane's animation speed
is applied to the ripple clock; reduced motion removes the ripple and still snaps the shape spring as
before.

This is a focused port of the filled button's shape, color, and bounded-ripple behavior, not full
Compose parity. The local theme maps Material's primary/on-primary to `theme.accent`/`theme.on_accent`;
this shell draws logical pixels rather than Android dp.

## Input support and limits

- Amane's `Rectangle::on_drag` begins on a left-button press and reports pointer movement; its
  `on_click` callback runs only after release over the same hit target. This module uses those real
  events: it does not treat a click callback as a press. Leaving the hit area clears the visible
  press, and a release over the enabled sample runs its action.
- There is no distinct pointer-release-outside or pointer-cancel callback for a control. A pointer
  leaving the hit area is therefore used to finish the visible press ripple; a release over the
  button finishes it through `on_click`. This is the closest available cancel signal, not a claim of
  AndroidX's complete interaction stream. Re-entering while a drag is still active starts a new
  ripple because Amane reports only the current drag point, not a separate press interaction.
- Keyboard events are window-level key presses. Amane does not expose per-button focus, a key-up
  event, or button accessibility semantics, so keyboard activation and focus styling are not
  implemented here.
- Amane has no native indication facility or circle-mask primitive. The bounded effect is drawn from
  the source timings and geometry above using a canvas circle clipped by Amane's rounded-rectangle
  group clip (`widgets/rectangle/draw.rs`); it is not AndroidX's indication node, and the 10 dp
  overflow token is represented as 10 logical pixels. The clip follows the button's current animated
  radius rather than a separately animated Material shape outline.
- Hover can be detected. The shell's shadow renderer is not a Material elevation implementation;
  the subtle hover shadow is an approximation of the filled-button hover elevation token.

To open the isolated panel, run `amane ipc call showcase`. It is not registered as a startup
window. The existing Settings panel and existing button call sites are not changed.

## Filled icon-button slice

This adds the expressive, filled, momentary `FilledIconButton` overload from
`compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/IconButton.kt` (the
`shapes` overload around lines 557-575). It does not add the toggle, tonal, outlined, or other-size
variants. The internal `IconButtonDefaults.shapes()` uses the round and pressed shapes from
`SmallIconButtonTokens`; its press morph uses the same DefaultEffects spring as the text button.

`SmallIconButtonTokens.kt` (`14_1_0`) sets a 40 dp container, 24 dp icon, 8 dp leading/trailing
space, full-round resting shape, and 8 dp pressed corners. `FilledIconButtonTokens.kt` (`14_1_0`)
maps the enabled container/icon to primary/on-primary and disabled colors to on-surface at 10%/38%.
The small `button::filled_icon(name, theme, icon_shape, enabled, on_activate)` interface accepts an
Amane `Shape`; the button module applies the enabled/disabled tint and uses its existing interaction,
spring, bounded ripple, and clip internals. It is momentary and does not claim selected/toggle state.

The checked-in `assets/settings_fill.svg` preserves the supplied 24 px dimensions, `0 -960 960 960`
viewBox, fill metadata, and path data. Amane can rasterize SVGs, but its `Image` interface has no tint
operation. To keep the icon themed, `settings_fill()` provides the same path as an Amane vector shape,
mapped to 24 logical pixels and filled by `filled_icon`; the SVG itself is not loaded for display.
The enabled and disabled showcase examples have separate stable names and activation counts; the
enabled gear action only increments its demo count and never opens Settings.

The filled icon uses the source's 8% hover state-layer color, shown immediately rather than using
AndroidX's 15 ms state-layer tween. Amane does not provide the 48 dp minimum touch-target expansion,
per-button focus/key-up events, or button accessibility role, so the hit target remains the 40 px
visual container and keyboard/focus indication are absent. Pointer leave is still the available
cancel signal described above; reduced motion suppresses the ripple and snaps the shape morph.

## Verification

The button module's 12 unit tests cover private press/release/cancel/repeat/disabled/reduced-motion
transitions, deterministic bounded-ripple origin/expansion/fade progress, separate logical
identities, the supplied SVG name/viewBox, deterministic spring retargeting, and the five analytical
spring cases. They do not exercise either public button function end-to-end: the Amane widget
callbacks, rendered clipping/colors/shadow, and activation timing are not covered by these private
tests. The repository has no `Cargo.toml`; after
`amane compile` generates the shell project manifest, run the component tests with:

```sh
cargo test --manifest-path "$HOME/.cache/amane/project/Cargo.toml" components::button
```
