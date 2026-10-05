# Material Color Utilities reference provenance

## Canonical source and generation

- Canonical repository: `https://github.com/material-foundation/material-color-utilities.git`.
- Package: `typescript/`, `@material/material-color-utilities` version `0.4.0`.
- Fixture generation used a clean checkout of the canonical repository at the pinned commit below.
- Source commit: `5b3618b16fdc3825e21d5679bafd144662088ea1` (`git status --porcelain` was empty).
- The nearby Rust `mcu-material-color` checkout was not used as a reference.
- Generation tool: Deno `2.9.7` (V8 `15.0.245.2-rusty`, TypeScript `6.0.3`). The runner imports the
  pinned TypeScript files directly with `--unstable-sloppy-imports` to resolve upstream `.js`
  specifiers to `.ts`; it does not compile or edit the upstream checkout. No npm package or network
  access was needed. `DENO_DIR` and intermediate output were isolated in a temporary directory.
- Generator: [`reference/generate_2026.ts`](reference/generate_2026.ts). It invokes upstream APIs;
  it does not reimplement color calculations.

To reproduce generation, set `MCU_REPO` to an existing clean checkout at the pinned commit above.
From the Amane repository root, run:

```sh
set -eu
MCU_REPO="${MCU_REPO:?Set MCU_REPO to a clean checkout at commit 5b3618b16fdc3825e21d5679bafd144662088ea1}"
MCU_REPO="$(cd "$MCU_REPO" && pwd -P)"
MCU_TS_ROOT="$MCU_REPO/typescript"
AMANE_ROOT="$PWD"
BUILD_DIR="$(mktemp -d)"
trap 'rm -rf "$BUILD_DIR"' EXIT
OUT="$BUILD_DIR/generated"
DENO_CACHE="$BUILD_DIR/deno-cache"
mkdir -p "$OUT" "$DENO_CACHE"
DENO_DIR="$DENO_CACHE" \
  deno run --unstable-sloppy-imports \
    --allow-read="$MCU_TS_ROOT,$AMANE_ROOT/algorithms/material_color/tests/reference" \
    --allow-write="$OUT" \
    "$AMANE_ROOT/algorithms/material_color/tests/reference/generate_2026.ts" \
    "$MCU_TS_ROOT" "$OUT"
cp "$OUT"/*.tsv "$AMANE_ROOT/algorithms/material_color/tests/fixtures/"
```

Two separate executions against the supplied pinned checkout produced byte-identical TSVs.

## Spec selection and fallback

The fixture requests `specVersion: "2026"` for every scheme. `DynamicScheme.maybeFallbackSpecVersion`
in the pinned source retains 2026 for CMF, maps Neutral/Tonal Spot/Vibrant/Expressive to 2025, and
maps Monochrome/Fidelity/Content/Rainbow/Fruit Salad to 2021. Each case stores the constructor's
actual `scheme.specVersion`, and outputs are generated with that effective upstream behavior. These
are 2026 requests with canonical internal fallbacks, not standalone 2021/2025 fixture matrices.

The normal matrix uses all ten `Variant` values, platforms `phone`/`watch`, light/dark, and contrast
levels `-1`, `-0.5`, `0`, `0.5`, `1`. CMF cases use the official `SchemeCmf` constructor so its source
list and generated palettes are exercised. Palette-override cases use the official
`DynamicScheme` options; a partial override is not used for CMF because its palette delegate cannot
derive the missing CMF defaults. CMF validation cases document the source-level 2026 guard, empty
source behavior, generic empty/missing-source checks, and the constructor's lack of contrast-range
validation.

## Source integrity and license

The TypeScript checkout was clean at the pinned commit. SHA-256 hashes for the central imported
sources at generation time:

| Upstream file | SHA-256 |
| --- | --- |
| `typescript/dynamiccolor/dynamic_scheme.ts` | `d525a66da4139c100ee8959d868aee26b31dba757a035180040ffc68fd415e3f` |
| `typescript/dynamiccolor/variant.ts` | `bdb7d29028943812c3aaced1f6efeb212b1a4a3b695222a50844b6e26436298d` |
| `typescript/dynamiccolor/material_dynamic_colors.ts` | `fcabf2d2fb75f52a0a72df0da5b49c5c8305250986bf4354ea466031b690b9ec` |
| `typescript/dynamiccolor/color_spec_2021.ts` | `af94991694a74ccd620b122b7f6ccb9f2076963850ee388167b344a9b73cbda9` |
| `typescript/dynamiccolor/color_spec_2025.ts` | `743a6c6235b4f955efc860712ca7e9da3790d960a32fbae16376dafd892068a9` |
| `typescript/dynamiccolor/color_spec_2026.ts` | `82587998b8df3c743e426089d128223d328d607083ce6d940f2cc2f5f750c533` |
| `typescript/dynamiccolor/dynamic_color.ts` | `418d22e153309a8f3a105606700e5c97f3440e5a597c16e4734bdfa0aab707c8` |
| `typescript/scheme/scheme_cmf.ts` | `57c2d395f09b0c95420e81191ba4273a08421afb076bc829d49dc20c0a54cac4` |
| `typescript/hct/hct.ts` | `8bfad96bc044dcdb89331bf37405c0186bec50a2caa3782000bdc95a0021abdb` |
| `typescript/hct/cam16.ts` | `bce01b5133a3cb59c5ebb6925b27c038162209ccb24c685319534103692906d5` |
| `typescript/palettes/tonal_palette.ts` | `12aee5a3ef54fdc571b91d9c0dc92619b64c2977705a2e504c5cc594cdcd0282` |
| `typescript/LICENSE` | `8ded460ad5bd3ab9082eaa2f7502cc9bb621eaff87cecc55d823f21758707cea` |

SHA-256 of the checked-in generator and TSV outputs:

| File | SHA-256 |
| --- | --- |
| `reference/generate_2026.ts` | `0abe947b014379a9a7c3cd8e8b38fe72e50d373947e021b986030cbdb12e3524` |
| `fixtures/dynamic_cases_2026.tsv` | `73a9f42332f379711a12f5442cf0e4f6bdb83d966a2aba0063b1440b10e9b975` |
| `fixtures/dynamic_roles_2026.tsv` | `3b44cc81e17fe6a68708d0d34bd73abc8dd6cf0f02bff42c5e4529f3a76f3fb3` |
| `fixtures/dynamic_edges_2026.tsv` | `6fc5f51ab698bd997963dd882b019ace752ca20c6a0f2ae3f3a8bc7a9c1764a7` |
| `fixtures/dynamic_role_manifest_2026.tsv` | `566965b6655aba6963dfb597e5cbcf0fade43a6aeb12b424292ca25c61e89e9b` |
| `fixtures/cmf_validity_2026.tsv` | `43028c24b9f60d15ee86b8b2e9f61adc90318da7ecce0c00418359a6faa678a5` |
| `fixtures/cmf_error_hue_boundaries_2026.tsv` | `503bbfd5d35525938a5db8c974742d7fc3e7985d005531b8dd69bd7a6f3e367e` |
| `fixtures/hct_vectors_2026.tsv` | `e5dc81d85f795043be9d63b45dc4a7ca86a531413243b790c682eda97520cb0e` |
| `fixtures/cam16_vectors_2026.tsv` | `fe3aa7da0e95810b40f790817225d6fd637b9311c1ad15e21a4b381574db3343` |
| `fixtures/tonal_palette_vectors_2026.tsv` | `38d012789cdffb462cb6ddeec0b3cac046006bc63f2486167ac863563259da97` |

The TypeScript package declares Apache-2.0. `reference/LICENSE` is a byte-for-byte copy of the
upstream TypeScript license. The upstream checkout contains no `NOTICE` file at the pinned commit,
so Apache 2.0 section 4(d) supplies no upstream NOTICE text to reproduce. `reference/NOTICE` adds
source attribution for these fixtures. The generator header and NOTICE identify the upstream
implementation; per-file copyright/license headers remain in the imported upstream source and were
not copied into this repository.
