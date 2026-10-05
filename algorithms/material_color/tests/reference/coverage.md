# Reference fixture coverage

These fixtures are generated from the official TypeScript implementation pinned in
[`../provenance.md`](../provenance.md). Every scheme requests spec `2026`. There are no standalone
2021 or 2025 fixture matrices; the effective `DynamicScheme.specVersion` is recorded because the
upstream constructor deliberately falls back for some variants.

## Dynamic schemes

The canonical `MaterialDynamicColors` instance prototype supplies the 60 accessors listed in
`dynamic_role_manifest_2026.tsv`; six are palette-key roles. The generator enumerates the source
methods instead of maintaining a guessed role list. ARGB output is unsigned, uppercase
`0xAARRGGBB`. Role tone uses JavaScript's shortest round-trippable decimal representation. An
undefined role is written as the literal `null`, not replaced with a fabricated color. No accessor
returned undefined in these sampled 2026/effective-fallback cases (`null_rows` is zero in the
manifest).

`dynamic_cases_2026.tsv` stores one row per input scheme. Role results are normalized into one row
per `(case_id, role)` in `dynamic_roles_2026.tsv` (standard/source-HCT matrix) and
`dynamic_edges_2026.tsv` (multi-source and palette-override cases). The source list in case metadata
uses `|` separators. Case IDs beginning `S` refer to the standard matrix; `E` IDs refer to edge
cases.

| Fixture | Data rows | Coverage |
| --- | ---: | --- |
| `dynamic_cases_2026.tsv` | 5,280 | 3,200 standard and 2,080 edge schemes with source/options/effective-spec metadata. |
| `dynamic_roles_2026.tsv` | 192,000 | 16 deterministic seeds × 10 variants × 2 platforms × 2 modes × 5 contrast levels × 60 roles. |
| `dynamic_edges_2026.tsv` | 124,800 | 2,080 edge schemes × 60 roles. |
| `dynamic_role_manifest_2026.tsv` | 60 | Stable accessor order, role category, and output/null counts. |

The 16 standard source cases are eight opaque ARGB inputs for black, white, gray, red, green, blue,
yellow, and purple, plus HCT-generated sources at requested hues 8, 16, 28, 40, 115, 152, 260, and
272 with chroma 120 and tone 50. For those HCT-generated sources, the case records the actual
quantized HCT values and ARGB.

The edge matrix contains four source-list cases (blue/yellow, blue/blue, blue/yellow/red, and
blue/blue/yellow). It tests each of the six `DynamicScheme` palette overrides individually for the
nine non-CMF variants, and all six overrides together for all ten variants. The CMF single-override
cases are intentionally absent: `SchemeCmf` supplies its own six palettes and has no override
parameters, while generic `DynamicScheme` cannot derive missing CMF palettes. The complete
six-palette override case is supported and included for CMF.

Each custom palette is made with `TonalPalette.fromHueAndChroma(hue, chroma)`:
`primaryPalette=(20,64)`, `secondaryPalette=(70,40)`, `tertiaryPalette=(140,52)`,
`neutralPalette=(260,8)`, `neutralVariantPalette=(300,16)`, and `errorPalette=(10,84)`. Case metadata
records which overrides were supplied; this list gives their exact inputs.

## CMF and color science

| Fixture | Data rows | Coverage |
| --- | ---: | --- |
| `cmf_validity_2026.tsv` | 12 | CMF constructor defaults, one/multiple sources, identical-source branch, 2026-only guard, empty-input behavior, generic source validation, and partial-CMF-override behavior. |
| `cmf_error_hue_boundaries_2026.tsv` | 520 | Canonical `SchemeCmf.getErrorHue` at and within 1e-6 of every primary/tertiary hue threshold. |
| `hct_vectors_2026.tsv` | 8 | HCT from each named ARGB sample and HCT round-trip ARGB. |
| `cam16_vectors_2026.tsv` | 12 | CAM16 from ARGB, JCh, and UCS conversions, including all returned coordinates. |
| `tonal_palette_vectors_2026.tsv` | 20 | Yellow and blue palette tones, including fractional tones and the yellow T99 special case alongside direct HCT output. |

`SchemeCmf` rejects a non-2026 spec. An empty CMF source list reaches the source's primary-source
derefence before the base `DynamicScheme` empty-list check; the fixture records the observed
exception class. The constructor documents contrast in `[-1, 1]` but does not reject an out-of-range
value; that is recorded as a constructor observation, not an endorsed input. See provenance for the
exact upstream branches and effective-spec fallback.
