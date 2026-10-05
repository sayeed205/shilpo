/*
 * Reference generator for official Material Color Utilities TypeScript.
 * It imports the pinned upstream implementation; no algorithm implementation is copied here.
 * See the adjacent LICENSE and NOTICE for source attribution.
 */

import {pathToFileURL} from "node:url";

const [sourceRootArg, outputDirArg] = Deno.args;
if (!sourceRootArg || !outputDirArg) {
  throw new Error(
    "usage: generate_2026.ts <upstream-typescript-dir> <output-dir>",
  );
}

const sourceRoot = sourceRootArg.replace(/\/+$/, "");
const outputDir = outputDirArg.replace(/\/+$/, "");
const loadUpstream = (path: string) =>
  import(pathToFileURL(`${sourceRoot}/${path}`).href);

const [
  {Hct},
  {Cam16},
  {TonalPalette},
  {DynamicScheme},
  {Variant},
  {MaterialDynamicColors},
  {SchemeMonochrome},
  {SchemeNeutral},
  {SchemeTonalSpot},
  {SchemeVibrant},
  {SchemeExpressive},
  {SchemeFidelity},
  {SchemeContent},
  {SchemeRainbow},
  {SchemeFruitSalad},
  {SchemeCmf},
] = await Promise.all([
  loadUpstream("hct/hct.ts"),
  loadUpstream("hct/cam16.ts"),
  loadUpstream("palettes/tonal_palette.ts"),
  loadUpstream("dynamiccolor/dynamic_scheme.ts"),
  loadUpstream("dynamiccolor/variant.ts"),
  loadUpstream("dynamiccolor/material_dynamic_colors.ts"),
  loadUpstream("scheme/scheme_monochrome.ts"),
  loadUpstream("scheme/scheme_neutral.ts"),
  loadUpstream("scheme/scheme_tonal_spot.ts"),
  loadUpstream("scheme/scheme_vibrant.ts"),
  loadUpstream("scheme/scheme_expressive.ts"),
  loadUpstream("scheme/scheme_fidelity.ts"),
  loadUpstream("scheme/scheme_content.ts"),
  loadUpstream("scheme/scheme_rainbow.ts"),
  loadUpstream("scheme/scheme_fruit_salad.ts"),
  loadUpstream("scheme/scheme_cmf.ts"),
]);

const SPEC_VERSION = "2026";
const PLATFORMS = ["phone", "watch"] as const;
const MODES = [
  {name: "light", isDark: false},
  {name: "dark", isDark: true},
] as const;
const CONTRASTS = [-1, -0.5, 0, 0.5, 1] as const;

type SchemeCtor = new (
  sourceColorOrList: unknown,
  isDark: boolean,
  contrastLevel: number,
  specVersion: string,
  platform: string,
) => any;

const variants = [
  {id: "monochrome", value: Variant.MONOCHROME, ctor: SchemeMonochrome},
  {id: "neutral", value: Variant.NEUTRAL, ctor: SchemeNeutral},
  {id: "tonal_spot", value: Variant.TONAL_SPOT, ctor: SchemeTonalSpot},
  {id: "vibrant", value: Variant.VIBRANT, ctor: SchemeVibrant},
  {id: "expressive", value: Variant.EXPRESSIVE, ctor: SchemeExpressive},
  {id: "fidelity", value: Variant.FIDELITY, ctor: SchemeFidelity},
  {id: "content", value: Variant.CONTENT, ctor: SchemeContent},
  {id: "rainbow", value: Variant.RAINBOW, ctor: SchemeRainbow},
  {id: "fruit_salad", value: Variant.FRUIT_SALAD, ctor: SchemeFruitSalad},
  {id: "cmf", value: Variant.CMF, ctor: SchemeCmf},
] as const;

const sourceSamples = [
  {id: "black", kind: "argb", create: () => Hct.fromInt(0xff000000)},
  {id: "white", kind: "argb", create: () => Hct.fromInt(0xffffffff)},
  {id: "gray", kind: "argb", create: () => Hct.fromInt(0xff808080)},
  {id: "red", kind: "argb", create: () => Hct.fromInt(0xffff0000)},
  {id: "green", kind: "argb", create: () => Hct.fromInt(0xff00ff00)},
  {id: "blue", kind: "argb", create: () => Hct.fromInt(0xff0000ff)},
  {id: "yellow", kind: "argb", create: () => Hct.fromInt(0xffffff00)},
  {id: "purple", kind: "argb", create: () => Hct.fromInt(0xff800080)},
  ...[8, 16, 28, 40, 115, 152, 260, 272].map((hue) => ({
    id: `hct_hue_${String(hue).padStart(3, "0")}`,
    kind: "hct_hue_boundary",
    create: () => Hct.from(hue, 120, 50),
  })),
];

const roleAccessors = Object.getOwnPropertyNames(
  MaterialDynamicColors.prototype,
).filter((name) =>
  name !== "constructor" &&
  typeof MaterialDynamicColors.prototype[name] === "function"
);
const paletteKeyRoles = new Set([
  "primaryPaletteKeyColor",
  "secondaryPaletteKeyColor",
  "tertiaryPaletteKeyColor",
  "neutralPaletteKeyColor",
  "neutralVariantPaletteKeyColor",
  "errorPaletteKeyColor",
]);

function argb(value: number): string {
  return `0x${(value >>> 0).toString(16).padStart(8, "0").toUpperCase()}`;
}

function float(value: number): string {
  if (!Number.isFinite(value)) {
    return String(value);
  }
  return String(value);
}

function cell(value: unknown): string {
  const result = String(value);
  if (result.includes("\t") || result.includes("\n") || result.includes("\r")) {
    throw new Error(`TSV cell contains a control separator: ${result}`);
  }
  return result;
}

function tsvRow(values: unknown[]): string {
  return values.map(cell).join("\t");
}

const caseHeader = [
  "case_id",
  "scenario",
  "variant",
  "requested_spec",
  "effective_spec",
  "platform",
  "mode",
  "contrast",
  "source_case",
  "source_kind",
  "source_count",
  "source_argb",
  "source_argbs",
  "source_hue",
  "source_chroma",
  "source_tone",
  "palette_overrides",
];
const roleHeader = [
  "case_id",
  "role_category",
  "role",
  "argb",
  "tone",
];

const caseRows: string[] = [tsvRow(caseHeader)];
const roleRows: string[] = [tsvRow(roleHeader)];
const edgeRows: string[] = [tsvRow(roleHeader)];
const roleNullCounts = new Map(roleAccessors.map((role) => [role, 0]));
let completedSchemes = 0;
let nullRoleRows = 0;
let coreCaseCounter = 0;
let edgeCaseCounter = 0;

function makeScheme(
  variant: typeof variants[number],
  sources: any[],
  isDark: boolean,
  contrast: number,
  platform: string,
  paletteOverrides: Record<string, unknown> = {},
): any {
  if (Object.keys(paletteOverrides).length > 0) {
    return new DynamicScheme({
      sourceColorHcts: sources,
      variant: variant.value,
      contrastLevel: contrast,
      isDark,
      platform,
      specVersion: SPEC_VERSION,
      ...paletteOverrides,
    });
  }
  const ctor = variant.ctor as unknown as SchemeCtor;
  return new ctor(sources, isDark, contrast, SPEC_VERSION, platform);
}

function appendSchemeRows(
  target: string[],
  metadata: {
    caseId: string;
    scenario: string;
    variant: typeof variants[number];
    platform: string;
    mode: string;
    contrast: number;
    sourceCase: string;
    sourceKind: string;
    sources: any[];
    overrides: string[];
    scheme: any;
  },
): void {
  const {scheme, variant, sources} = metadata;
  const first = sources[0];
  const sourceArgbList = sources.map((source) => argb(source.toInt())).join("|");
  const colors = scheme.colors;

  caseRows.push(tsvRow([
    metadata.caseId,
    metadata.scenario,
    variant.id,
    SPEC_VERSION,
    scheme.specVersion,
    metadata.platform,
    metadata.mode,
    float(metadata.contrast),
    metadata.sourceCase,
    metadata.sourceKind,
    sources.length,
    argb(first.toInt()),
    sourceArgbList,
    float(first.hue),
    float(first.chroma),
    float(first.tone),
    metadata.overrides.length === 0 ? "none" : metadata.overrides.join(","),
  ]));

  for (const role of roleAccessors) {
    const accessor = colors[role];
    const dynamicColor = role === "highestSurface"
      ? accessor.call(colors, scheme)
      : accessor.call(colors);

    let outputArgb = "null";
    let outputTone = "null";
    if (dynamicColor !== undefined) {
      outputArgb = argb(dynamicColor.getArgb(scheme));
      outputTone = float(dynamicColor.getTone(scheme));
    } else {
      nullRoleRows++;
      roleNullCounts.set(role, (roleNullCounts.get(role) ?? 0) + 1);
    }

    target.push(tsvRow([
      metadata.caseId,
      paletteKeyRoles.has(role) ? "palette_key" : "dynamic_role",
      role,
      outputArgb,
      outputTone,
    ]));
  }
  completedSchemes++;
}

function appendMatrix(
  target: string[],
  scenario: string,
  sourceCase: string,
  sourceKind: string,
  createSources: () => any[],
  usePaletteOverrides = false,
  overrides: Record<string, unknown> = {},
  overrideNames: string[] = [],
): void {
  for (const variant of variants) {
    // SchemeCmf supplies all six palettes itself. DynamicScheme's CMF path
    // cannot derive any missing CMF palette, so partial override cases apply
    // only to variants whose upstream palette delegate supplies defaults.
    if (
      usePaletteOverrides && variant.id === "cmf" &&
      Object.keys(overrides).length < paletteNames.length
    ) {
      continue;
    }
    for (const platform of PLATFORMS) {
      for (const mode of MODES) {
        for (const contrast of CONTRASTS) {
          const sources = createSources();
          const scheme = makeScheme(
            variant,
            sources,
            mode.isDark,
            contrast,
            platform,
            usePaletteOverrides ? overrides : {},
          );
          const isCoreCase = target === roleRows;
          const caseNumber = isCoreCase ? ++coreCaseCounter : ++edgeCaseCounter;
          const caseId = `${isCoreCase ? "S" : "E"}${String(caseNumber).padStart(4, "0")}`;
          appendSchemeRows(target, {
            caseId,
            scenario,
            variant,
            platform,
            mode: mode.name,
            contrast,
            sourceCase,
            sourceKind,
            sources,
            overrides: overrideNames,
            scheme,
          });
        }
      }
    }
  }
}

for (const source of sourceSamples) {
  appendMatrix(
    roleRows,
    source.kind === "argb" ? "single_argb_source" : "hct_hue_boundary_source",
    source.id,
    source.kind,
    () => [source.create()],
  );
}

const multiSourceScenarios = [
  {
    id: "two_distinct_sources",
    sources: [0xff0000ff, 0xffffff00],
  },
  {
    id: "two_identical_sources",
    sources: [0xff0000ff, 0xff0000ff],
  },
  {
    id: "three_sources_first_secondary_distinct",
    sources: [0xff0000ff, 0xffffff00, 0xffff0000],
  },
  {
    id: "three_sources_first_secondary_same",
    sources: [0xff0000ff, 0xff0000ff, 0xffffff00],
  },
];
for (const scenario of multiSourceScenarios) {
  appendMatrix(
    edgeRows,
    scenario.id,
    scenario.id,
    "source_argb_list",
    () => scenario.sources.map((value) => Hct.fromInt(value)),
  );
}

const paletteInputs = {
  primaryPalette: [20, 64],
  secondaryPalette: [70, 40],
  tertiaryPalette: [140, 52],
  neutralPalette: [260, 8],
  neutralVariantPalette: [300, 16],
  errorPalette: [10, 84],
} as const;
const paletteNames = Object.keys(paletteInputs) as Array<keyof typeof paletteInputs>;
const allPalettes = Object.fromEntries(
  paletteNames.map((name) => {
    const [hue, chroma] = paletteInputs[name];
    return [name, TonalPalette.fromHueAndChroma(hue, chroma)];
  }),
);

for (const paletteName of paletteNames) {
  const [hue, chroma] = paletteInputs[paletteName];
  appendMatrix(
    edgeRows,
    `override_${paletteName}`,
    "blue_yellow_sources",
    "source_argb_list",
    () => [Hct.fromInt(0xff0000ff), Hct.fromInt(0xffffff00)],
    true,
    {[paletteName]: TonalPalette.fromHueAndChroma(hue, chroma)},
    [paletteName],
  );
}
appendMatrix(
  edgeRows,
  "override_all_palettes",
  "single_blue_source",
  "source_argb_list",
  () => [Hct.fromInt(0xff0000ff)],
  true,
  allPalettes,
  paletteNames,
);

const colorSeeds = [
  ["black", 0xff000000],
  ["white", 0xffffffff],
  ["gray", 0xff808080],
  ["red", 0xffff0000],
  ["green", 0xff00ff00],
  ["blue", 0xff0000ff],
  ["yellow", 0xffffff00],
  ["purple", 0xff800080],
] as const;

const hctRows: string[] = [tsvRow([
  "case_id",
  "input_argb",
  "hue",
  "chroma",
  "tone",
  "round_trip_argb",
])];
for (const [name, input] of colorSeeds) {
  const hct = Hct.fromInt(input);
  hctRows.push(tsvRow([
    `from_int_${name}`,
    argb(input),
    float(hct.hue),
    float(hct.chroma),
    float(hct.tone),
    argb(Hct.from(hct.hue, hct.chroma, hct.tone).toInt()),
  ]));
}

const camRows: string[] = [tsvRow([
  "case_id",
  "kind",
  "input_argb",
  "input_j",
  "input_chroma",
  "input_hue",
  "hue",
  "chroma",
  "j",
  "q",
  "m",
  "s",
  "jstar",
  "astar",
  "bstar",
  "output_argb",
])];
function appendCamRow(
  caseId: string,
  kind: string,
  cam: any,
  inputArgb: string,
  inputJ: string,
  inputChroma: string,
  inputHue: string,
): void {
  camRows.push(tsvRow([
    caseId,
    kind,
    inputArgb,
    inputJ,
    inputChroma,
    inputHue,
    float(cam.hue),
    float(cam.chroma),
    float(cam.j),
    float(cam.q),
    float(cam.m),
    float(cam.s),
    float(cam.jstar),
    float(cam.astar),
    float(cam.bstar),
    argb(cam.toInt()),
  ]));
}
for (const [name, input] of colorSeeds) {
  appendCamRow(
    `from_int_${name}`,
    "from_int_default_viewing_conditions",
    Cam16.fromInt(input),
    argb(input),
    "null",
    "null",
    "null",
  );
}
for (const [name, j, chroma, hue] of [
  ["jch_red", 50, 60, 30],
  ["jch_blue", 60, 45, 250],
  ["jch_neutral", 75, 2, 203],
] as const) {
  appendCamRow(
    name,
    "from_jch_default_viewing_conditions",
    Cam16.fromJch(j, chroma, hue),
    "null",
    float(j),
    float(chroma),
    float(hue),
  );
}
const camUcsSource = Cam16.fromInt(0xff6750a4);
appendCamRow(
  "ucs_round_trip_material_purple",
  "from_ucs_default_viewing_conditions",
  Cam16.fromUcs(camUcsSource.jstar, camUcsSource.astar, camUcsSource.bstar),
  "0xFF6750A4",
  "null",
  "null",
  "null",
);

const tonalRows: string[] = [tsvRow([
  "case_id",
  "palette_hue",
  "palette_chroma",
  "requested_tone",
  "is_yellow_hue",
  "key_color_argb",
  "key_color_tone",
  "tone_argb",
  "direct_hct_argb",
  "tone_98_argb",
  "tone_100_argb",
])];
for (const paletteInput of [
  {name: "yellow", hue: 115, chroma: 120},
  {name: "blue", hue: 260, chroma: 80},
]) {
  const palette = TonalPalette.fromHueAndChroma(
    paletteInput.hue,
    paletteInput.chroma,
  );
  for (const tone of [0, 1, 49.5, 50, 50.25, 98, 98.5, 99, 99.5, 100]) {
    tonalRows.push(tsvRow([
      `${paletteInput.name}_tone_${String(tone).replace(".", "p")}`,
      float(palette.hue),
      float(palette.chroma),
      float(tone),
      Hct.isYellow(palette.hue),
      argb(palette.keyColor.toInt()),
      float(palette.keyColor.tone),
      argb(palette.tone(tone)),
      argb(Hct.from(palette.hue, palette.chroma, tone).toInt()),
      argb(palette.tone(98)),
      argb(palette.tone(100)),
    ]));
  }
}

const cmfValidityRows: string[] = [tsvRow([
  "case_id",
  "constructor",
  "requested_spec",
  "source_form",
  "source_argbs",
  "source_count",
  "platform",
  "mode",
  "contrast",
  "outcome",
  "error_name",
  "effective_spec",
  "primary_palette_hue",
  "tertiary_palette_hue",
  "error_palette_hue",
  "note",
])];
const blueHct = Hct.fromInt(0xff0000ff);
const yellowHct = Hct.fromInt(0xffffff00);
const redHct = Hct.fromInt(0xffff0000);
const cmfCases = [
  {
    id: "cmf_default_scalar_source",
    spec: undefined,
    sources: [blueHct],
    form: "scalar",
    contrast: 0,
    note: "The constructor default is 2026.",
  },
  {
    id: "cmf_2026_singleton_list",
    spec: "2026",
    sources: [blueHct],
    form: "list",
    contrast: 0,
    note: "One-element source list is accepted.",
  },
  {
    id: "cmf_2026_distinct_pair",
    spec: "2026",
    sources: [blueHct, yellowHct],
    form: "list",
    contrast: 0,
    note: "First extra source supplies tertiary hue/chroma when distinct.",
  },
  {
    id: "cmf_2026_identical_pair",
    spec: "2026",
    sources: [blueHct, blueHct],
    form: "list",
    contrast: 0,
    note: "Equal first two source ARGB values select the source-derived tertiary chroma.",
  },
  {
    id: "cmf_2026_three_sources",
    spec: "2026",
    sources: [blueHct, yellowHct, redHct],
    form: "list",
    contrast: 0,
    note: "All sources remain on the scheme; palette derivation consults the first extra source.",
  },
  {
    id: "cmf_2026_third_source_ignored_for_palette_choice",
    spec: "2026",
    sources: [blueHct, blueHct, yellowHct],
    form: "list",
    contrast: 0,
    note: "The first extra source equals primary; the later source does not replace it.",
  },
  {
    id: "cmf_out_of_domain_contrast_is_not_constructor_rejected",
    spec: "2026",
    sources: [blueHct],
    form: "scalar",
    contrast: 2,
    note: "Constructor documents -1..1 but does not validate contrast range.",
  },
  {
    id: "cmf_non_2026_spec_rejected",
    spec: "2025",
    sources: [blueHct],
    form: "scalar",
    contrast: 0,
    note: "Negative guard only; no legacy-spec output matrix is generated.",
  },
  {
    id: "cmf_empty_source_list_dereferences_missing_primary",
    spec: "2026",
    sources: [],
    form: "list",
    contrast: 0,
    note: "Source reads sourceColorOrList[0] before DynamicScheme's empty-list validation.",
  },
] as const;
for (const testCase of cmfCases) {
  let outcome = "accepted";
  let errorName = "null";
  let effectiveSpec = "null";
  let primaryPaletteHue = "null";
  let tertiaryPaletteHue = "null";
  let errorPaletteHue = "null";
  let sourceCount = testCase.sources.length;
  try {
    const sources = [...testCase.sources];
    const sourceArgument = testCase.form === "scalar" ? sources[0] : sources;
    const scheme = new SchemeCmf(
      sourceArgument,
      false,
      testCase.contrast,
      testCase.spec,
      "phone",
    );
    effectiveSpec = scheme.specVersion;
    sourceCount = scheme.sourceColorHcts.length;
    primaryPaletteHue = float(scheme.primaryPalette.hue);
    tertiaryPaletteHue = float(scheme.tertiaryPalette.hue);
    errorPaletteHue = float(scheme.errorPalette.hue);
  } catch (error) {
    outcome = "throws";
    errorName = error instanceof Error ? error.name : typeof error;
  }
  cmfValidityRows.push(tsvRow([
    testCase.id,
    "SchemeCmf",
    testCase.spec ?? "default",
    testCase.form,
    testCase.sources.map((source) => argb(source.toInt())).join("|") || "none",
    sourceCount,
    "phone",
    "light",
    float(testCase.contrast),
    outcome,
    errorName,
    effectiveSpec,
    primaryPaletteHue,
    tertiaryPaletteHue,
    errorPaletteHue,
    testCase.note,
  ]));
}

for (const [id, attempt] of [
  [
    "generic_dynamic_scheme_empty_source_list",
    () => new DynamicScheme({
      sourceColorHcts: [],
      variant: Variant.CMF,
      contrastLevel: 0,
      isDark: false,
      specVersion: SPEC_VERSION,
    }),
  ],
  [
    "generic_dynamic_scheme_missing_source",
    () => new DynamicScheme({
      variant: Variant.CMF,
      contrastLevel: 0,
      isDark: false,
      specVersion: SPEC_VERSION,
    }),
  ],
  [
    "generic_cmf_with_partial_palette_override",
    () => new DynamicScheme({
      sourceColorHcts: [blueHct],
      variant: Variant.CMF,
      contrastLevel: 0,
      isDark: false,
      specVersion: SPEC_VERSION,
      primaryPalette: TonalPalette.fromHueAndChroma(20, 64),
    }),
  ],
] as const) {
  let outcome = "accepted";
  let errorName = "null";
  try {
    attempt();
  } catch (error) {
    outcome = "throws";
    errorName = error instanceof Error ? error.name : typeof error;
  }
  cmfValidityRows.push(tsvRow([
    id,
    "DynamicScheme",
    SPEC_VERSION,
    id.includes("empty") ? "empty_list" : "missing",
    "none",
    0,
    "phone",
    "light",
    "0",
    outcome,
    errorName,
    "null",
    "null",
    "null",
    "null",
    id === "generic_cmf_with_partial_palette_override"
      ? "CMF needs all six palettes supplied; use SchemeCmf for its canonical defaults."
      : "Base DynamicScheme source validation.",
  ]));
}

const cmfErrorRows: string[] = [tsvRow([
  "primary_hue",
  "tertiary_hue",
  "error_hue",
])];
const boundary = (thresholds: number[]) => {
  const values = new Set<number>([0, 359.999999]);
  for (const threshold of thresholds) {
    values.add(Number((threshold - 0.000001).toFixed(6)));
    values.add(threshold);
    values.add(Number((threshold + 0.000001).toFixed(6)));
  }
  return [...values].filter((value) => value >= 0 && value < 360).sort((a, b) => a - b);
};
for (const primaryHue of boundary([8, 16, 20, 28, 32, 40, 152, 272])) {
  for (const tertiaryHue of boundary([12, 20, 24, 28, 32, 36])) {
    cmfErrorRows.push(tsvRow([
      float(primaryHue),
      float(tertiaryHue),
      float(SchemeCmf.getErrorHue(primaryHue, tertiaryHue)),
    ]));
  }
}

const roleManifestRows: string[] = [tsvRow([
  "order",
  "role",
  "role_category",
  "output_rows",
  "null_rows",
])];
for (const [index, role] of roleAccessors.entries()) {
  roleManifestRows.push(tsvRow([
    index,
    role,
    paletteKeyRoles.has(role) ? "palette_key" : "dynamic_role",
    completedSchemes,
    roleNullCounts.get(role) ?? 0,
  ]));
}

await Deno.mkdir(outputDir, {recursive: true});
const outputs = [
  ["dynamic_cases_2026.tsv", caseRows],
  ["dynamic_roles_2026.tsv", roleRows],
  ["dynamic_edges_2026.tsv", edgeRows],
  ["dynamic_role_manifest_2026.tsv", roleManifestRows],
  ["hct_vectors_2026.tsv", hctRows],
  ["cam16_vectors_2026.tsv", camRows],
  ["tonal_palette_vectors_2026.tsv", tonalRows],
  ["cmf_validity_2026.tsv", cmfValidityRows],
  ["cmf_error_hue_boundaries_2026.tsv", cmfErrorRows],
] as const;
for (const [name, rows] of outputs) {
  await Deno.writeTextFile(`${outputDir}/${name}`, `${rows.join("\n")}\n`);
}

console.log(JSON.stringify({
  sourceRoot,
  specRequested: SPEC_VERSION,
  variants: variants.map((variant) => variant.id),
  platforms: PLATFORMS,
  modes: MODES.map((mode) => mode.name),
  contrasts: CONTRASTS,
  seedCases: sourceSamples.length,
  roleAccessors,
  roleAccessorCount: roleAccessors.length,
  roleNullCounts: Object.fromEntries(roleNullCounts),
  completedSchemes,
  roleOutputRows: roleRows.length - 1,
  edgeOutputRows: edgeRows.length - 1,
  nullRoleRows,
  cmfErrorHueCases: cmfErrorRows.length - 1,
  outputs: outputs.map(([name, rows]) => ({name, rows: rows.length - 1})),
}, null, 2));
