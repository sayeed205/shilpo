# Standalone Material Color module

This is a dependency-free Rust module for generating Material dynamic color
schemes and tonal palettes. It is not a Cargo crate and does not register with
or change Amane's runtime, UI, or shell color handling.

## Quick start

From a Rust source file at the Amane repository root, include `lib.rs` as a
module. Its internal paths expect the module name `crate::material_color`:

```rust
#[path = "algorithms/material_color/lib.rs"]
mod material_color;

use material_color::{Platform, Role, Scheme, SchemeOptions, Variant};

fn main() -> Result<(), material_color::SchemeError> {
    let scheme = Scheme::new(
        &[0xff6750a4],
        SchemeOptions::new(Variant::TonalSpot, false)
            .with_contrast_level(0.0)
            .with_platform(Platform::Phone),
    )?;

    assert!(matches!(scheme.role(Role::Primary), Some(_)));
    println!("primary: {:#010x}", scheme.primary_palette().tone_at(42.5));
    Ok(())
}
```

`Scheme::new` accepts one or more 32-bit ARGB source colors in order. Use the
first as the primary seed; CMF also uses a second source as its tertiary seed.
Choose light or dark with the second `SchemeOptions::new` argument. Contrast is
forwarded without clamping (the standard level is `0.0`), and `Platform` is
`Phone` or `Watch`. The ten `Variant` choices are `Monochrome`, `Neutral`,
`TonalSpot`, `Vibrant`, `Expressive`, `Fidelity`, `Content`, `Rainbow`,
`FruitSalad`, and `Cmf`. Resolve a role to `Option<u32>` with `scheme.role(Role::Primary)`.

Every request uses the 2026 entry point; there is no public legacy-spec
selector. Canonical per-variant fallback is internal: CMF uses effective 2026;
Neutral, TonalSpot, Vibrant, and Expressive use 2025; Monochrome, Fidelity,
Content, Rainbow, and FruitSalad use 2021. `scheme.effective_spec()` reports
that effective specification.

For standalone palettes, use `Palette::from_int(argb)` or
`Palette::from_hue_and_chroma(hue, chroma)`, then call `tone_at(42.5)` for an
exact fractional tone (or `tone(42)` for an integral tone). The HCT implementation
is intentionally private; use ARGB, hue/chroma, tones, and the scalar
`key_color_argb()` accessor at the module boundary.

`SchemeOptions` supports six palette overrides through
`with_primary_palette`, `with_secondary_palette`, `with_tertiary_palette`,
`with_neutral_palette`, `with_neutral_variant_palette`, and
`with_error_palette` ([API source](options.rs#L70-L97)). Omitted palettes are
generated normally; CMF overrides must supply all six or none.

## Validation and reference scope

Run the standalone tests from the Amane repository root (no Cargo dependencies
are required):

```sh
set -eu
BUILD_DIR="$(mktemp -d)"
trap 'rm -rf "$BUILD_DIR"' EXIT
rustc --edition=2021 --test algorithms/material_color/tests/harness.rs \
  -o "$BUILD_DIR/material-color-tests"
"$BUILD_DIR/material-color-tests"
```

The tests compare finite scheme/reference matrices and a bounded ARGB-alpha
matrix against the pinned official TypeScript implementation. See
[`tests/provenance.md`](tests/provenance.md),
[`tests/reference/coverage.md`](tests/reference/coverage.md), and
[`tests/reference/alpha_coverage.md`](tests/reference/alpha_coverage.md) for
source pins and coverage limits. The last verified standalone build reported
23 compiler warnings. These finite checks are not a guarantee of universal
parity for every color or option. This module does not decode images, quantize
or score colors, or provide application/runtime integration.

The code is licensed under Apache-2.0; see [`LICENSE`](LICENSE) and
[`NOTICE`](NOTICE) for attribution.
