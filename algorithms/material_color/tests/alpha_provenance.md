# Alpha reference provenance

## Canonical source and preflight

- Canonical repository: `https://github.com/material-foundation/material-color-utilities.git`.
- Package: `typescript/`, `@material/material-color-utilities` version `0.4.0`.
- Generation used a clean checkout of the canonical repository at the required commit below.
- Required commit: `5b3618b16fdc3825e21d5679bafd144662088ea1`.
- The Deno generator refuses another commit, a dirty upstream checkout, a non-matching TypeScript
  source root, a changed original role manifest, any listed source hash mismatch, or tool versions
  other than Deno `2.9.7` with embedded TypeScript `6.0.3`.
- Generation imports the pinned TypeScript implementation directly using Deno's
  `--unstable-sloppy-imports` resolver for upstream `.js` specifiers. It installs no npm package,
  accesses no network, writes nothing to the upstream checkout, and contains no copied color
  algorithm implementation.
- The original `dynamic_role_manifest_2026.tsv` is read to assert the runtime's 60 role accessors
  and their exact order. Its SHA-256 is
  `566965b6655aba6963dfb597e5cbcf0fade43a6aeb12b424292ca25c61e89e9b`.
- Original finalized fixtures, original provenance, original generator, license, and notice are not
  generator outputs and were not edited. A SHA-256 baseline of all 14 original files was captured
  outside the repository before the supplement and checked after generation.

## Generation command

From the Amane repository root, set `MCU_REPO` to a local clean checkout of the canonical
repository at the required commit. The command uses a temporary workspace and does not clone or
download upstream source:

```sh
set -eu
MCU_REPO="${MCU_REPO:?Set MCU_REPO to a clean checkout at commit 5b3618b16fdc3825e21d5679bafd144662088ea1}"
MCU_REPO="$(cd "$MCU_REPO" && pwd -P)"
TS_ROOT="$MCU_REPO/typescript"
AMANE_ROOT="$PWD"
BUILD_DIR="$(mktemp -d)"
trap 'rm -rf "$BUILD_DIR"' EXIT
OUT_A="$BUILD_DIR/alpha-a"
OUT_B="$BUILD_DIR/alpha-b"
DENO_CACHE="$BUILD_DIR/deno-cache"
mkdir -p "$DENO_CACHE" "$OUT_A" "$OUT_B"
for OUT in "$OUT_A" "$OUT_B"; do
  DENO_DIR="$DENO_CACHE" \
    deno run --unstable-sloppy-imports --allow-run=git \
      --allow-read="$MCU_REPO,$AMANE_ROOT" --allow-write="$OUT" \
      "$AMANE_ROOT/algorithms/material_color/tests/reference/generate_alpha_2026.ts" \
      "$TS_ROOT" "$OUT" "$MCU_REPO" "$AMANE_ROOT"
done
diff -qr "$OUT_A" "$OUT_B"
cp "$OUT_A"/alpha_*.tsv "$AMANE_ROOT/algorithms/material_color/tests/fixtures/"
```

Both independent executions passed preflight and produced byte-identical TSV files.

## Verified upstream source hashes

The generator checks these source and license hashes before importing the implementation:

| Upstream file | SHA-256 |
| --- | --- |
| `typescript/dynamiccolor/dynamic_scheme.ts` | `d525a66da4139c100ee8959d868aee26b31dba757a035180040ffc68fd415e3f` |
| `typescript/dynamiccolor/variant.ts` | `bdb7d29028943812c3aaced1f6efeb212b1a4a3b695222a50844b6e26436298d` |
| `typescript/dynamiccolor/material_dynamic_colors.ts` | `fcabf2d2fb75f52a0a72df0da5b49c5c8305250986bf4354ea466031b690b9ec` |
| `typescript/dynamiccolor/color_spec.ts` | `146ad93f0b896f384abd5fbb81b276b7e85b338b75784a3373f2f208a9a59b37` |
| `typescript/dynamiccolor/color_spec_2021.ts` | `af94991694a74ccd620b122b7f6ccb9f2076963850ee388167b344a9b73cbda9` |
| `typescript/dynamiccolor/color_spec_2025.ts` | `743a6c6235b4f955efc860712ca7e9da3790d960a32fbae16376dafd892068a9` |
| `typescript/dynamiccolor/color_spec_2026.ts` | `82587998b8df3c743e426089d128223d328d607083ce6d940f2cc2f5f750c533` |
| `typescript/dynamiccolor/dynamic_color.ts` | `418d22e153309a8f3a105606700e5c97f3440e5a597c16e4734bdfa0aab707c8` |
| `typescript/dynamiccolor/contrast_curve.ts` | `9a7ce1216f2b00ef17d22e34db97d37920216e3729bc0165f0dbf669cb357931` |
| `typescript/dynamiccolor/tone_delta_pair.ts` | `57030b8ddbe9c31bb1a3af5122306f525aa08aefd02725e63a67fdee3f6d928b` |
| `typescript/scheme/scheme_cmf.ts` | `57c2d395f09b0c95420e81191ba4273a08421afb076bc829d49dc20c0a54cac4` |
| `typescript/hct/hct.ts` | `8bfad96bc044dcdb89331bf37405c0186bec50a2caa3782000bdc95a0021abdb` |
| `typescript/hct/cam16.ts` | `bce01b5133a3cb59c5ebb6925b27c038162209ccb24c685319534103692906d5` |
| `typescript/hct/hct_solver.ts` | `a6257b360da1438015be8c8282506bef1adf1da3e6d86e920491916469f3508c` |
| `typescript/hct/viewing_conditions.ts` | `95897551c39fd4d1d04db495751b07365cf116a6a5b7261a716da9cc8c673b21` |
| `typescript/palettes/tonal_palette.ts` | `12aee5a3ef54fdc571b91d9c0dc92619b64c2977705a2e504c5cc594cdcd0282` |
| `typescript/utils/math_utils.ts` | `7b4166ad3555848ce759ef8fb9888f0f663886093016e4d932d393d9c7779573` |
| `typescript/utils/color_utils.ts` | `0b2f0655da3bfa74bd56c75f1670679b22518ded0b52b3cbeffe531c036a681b` |
| `typescript/dislike/dislike_analyzer.ts` | `a5a5bb6f5b7d2d2a6c4740eb556832e0b3ec63a157ac606d60573e203fdefd4b` |
| `typescript/temperature/temperature_cache.ts` | `bddbc45322f43d74de019affc779ff9ed8bccf9254aca31fcc214444cbfdeb0a` |
| `typescript/LICENSE` | `8ded460ad5bd3ab9082eaa2f7502cc9bb621eaff87cecc55d823f21758707cea` |

SHA-256 of the checked-in supplement generator and fixtures:

| File | SHA-256 |
| --- | --- |
| `reference/generate_alpha_2026.ts` | `29f043118822cb66184d70f179b503e18e5b6a8243f70bcbe1cc6bd7fa0aa7f3` |
| `fixtures/alpha_cases_2026.tsv` | `bc5d49e92d7f1ddd860933e806819cb4bd1eee455583e331b6e4a72f445a97a8` |
| `fixtures/alpha_roles_2026.tsv` | `4f3b734e5185b01121cb9a2ff774573a846b041ac29981ec4ec1c25db3f951cb` |
| `fixtures/alpha_science_2026.tsv` | `5c0cee9d4a679586ce8d7bf011932dd463b596479cfde60b4a0856e7d358bbb4` |

The TypeScript package declares Apache-2.0. The parent reference directory includes the upstream
license copy and attribution notice; the new generator carries an Apache-2.0 SPDX header.

## Scope

This supplement covers only the explicit RGB seeds, alpha bytes, 2026 requests, variants, platforms,
modes, contrast zero, and same-RGB CMF pairs documented in
[`reference/alpha_coverage.md`](reference/alpha_coverage.md). It is intended as an independent
reference for alpha-specific parity cases, not as an exhaustive statement about all source colors,
alpha values, color extraction, or every Dynamic Color option.
