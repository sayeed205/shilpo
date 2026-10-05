/*
 * Copyright 2026 Material Color Utilities fixture contributors.
 * Alpha-reference supplement generator; executes the pinned upstream MCU TypeScript source.
 * SPDX-License-Identifier: Apache-2.0
 * See ../LICENSE and ../NOTICE for attribution and license terms.
 */

import {pathToFileURL} from "node:url";

const [sourceRootArg, outputDirArg, upstreamRepoRootArg, consumerRepoRootArg] = Deno.args;
if (!sourceRootArg || !outputDirArg || !upstreamRepoRootArg || !consumerRepoRootArg) {
  throw new Error(
    "usage: generate_alpha_2026.ts <upstream-typescript-dir> <output-dir> <upstream-repo-root> <consumer-repo-root>",
  );
}

const EXPECTED_COMMIT = "5b3618b16fdc3825e21d5679bafd144662088ea1";
const EXPECTED_DENO = "2.9.7";
const EXPECTED_TYPESCRIPT = "6.0.3";
const EXPECTED_ROLE_MANIFEST_SHA256 =
  "566965b6655aba6963dfb597e5cbcf0fade43a6aeb12b424292ca25c61e89e9b";
const SPEC_VERSION = "2026";
const SOURCE_HASHES: Record<string, string> = {
  "dynamiccolor/dynamic_scheme.ts":
    "d525a66da4139c100ee8959d868aee26b31dba757a035180040ffc68fd415e3f",
  "dynamiccolor/variant.ts":
    "bdb7d29028943812c3aaced1f6efeb212b1a4a3b695222a50844b6e26436298d",
  "dynamiccolor/material_dynamic_colors.ts":
    "fcabf2d2fb75f52a0a72df0da5b49c5c8305250986bf4354ea466031b690b9ec",
  "dynamiccolor/color_spec.ts":
    "146ad93f0b896f384abd5fbb81b276b7e85b338b75784a3373f2f208a9a59b37",
  "dynamiccolor/color_spec_2021.ts":
    "af94991694a74ccd620b122b7f6ccb9f2076963850ee388167b344a9b73cbda9",
  "dynamiccolor/color_spec_2025.ts":
    "743a6c6235b4f955efc860712ca7e9da3790d960a32fbae16376dafd892068a9",
  "dynamiccolor/color_spec_2026.ts":
    "82587998b8df3c743e426089d128223d328d607083ce6d940f2cc2f5f750c533",
  "dynamiccolor/dynamic_color.ts":
    "418d22e153309a8f3a105606700e5c97f3440e5a597c16e4734bdfa0aab707c8",
  "dynamiccolor/contrast_curve.ts":
    "9a7ce1216f2b00ef17d22e34db97d37920216e3729bc0165f0dbf669cb357931",
  "dynamiccolor/tone_delta_pair.ts":
    "57030b8ddbe9c31bb1a3af5122306f525aa08aefd02725e63a67fdee3f6d928b",
  "scheme/scheme_cmf.ts":
    "57c2d395f09b0c95420e81191ba4273a08421afb076bc829d49dc20c0a54cac4",
  "hct/hct.ts":
    "8bfad96bc044dcdb89331bf37405c0186bec50a2caa3782000bdc95a0021abdb",
  "hct/cam16.ts":
    "bce01b5133a3cb59c5ebb6925b27c038162209ccb24c685319534103692906d5",
  "hct/hct_solver.ts":
    "a6257b360da1438015be8c8282506bef1adf1da3e6d86e920491916469f3508c",
  "hct/viewing_conditions.ts":
    "95897551c39fd4d1d04db495751b07365cf116a6a5b7261a716da9cc8c673b21",
  "palettes/tonal_palette.ts":
    "12aee5a3ef54fdc571b91d9c0dc92619b64c2977705a2e504c5cc594cdcd0282",
  "utils/math_utils.ts":
    "7b4166ad3555848ce759ef8fb9888f0f663886093016e4d932d393d9c7779573",
  "utils/color_utils.ts":
    "0b2f0655da3bfa74bd56c75f1670679b22518ded0b52b3cbeffe531c036a681b",
  "dislike/dislike_analyzer.ts":
    "a5a5bb6f5b7d2d2a6c4740eb556832e0b3ec63a157ac606d60573e203fdefd4b",
  "temperature/temperature_cache.ts":
    "bddbc45322f43d74de019affc779ff9ed8bccf9254aca31fcc214444cbfdeb0a",
  "LICENSE":
    "8ded460ad5bd3ab9082eaa2f7502cc9bb621eaff87cecc55d823f21758707cea",
};

if (Deno.version.deno !== EXPECTED_DENO) {
  throw new Error(`Expected Deno ${EXPECTED_DENO}, got ${Deno.version.deno}`);
}
if (Deno.version.typescript !== EXPECTED_TYPESCRIPT) {
  throw new Error(
    `Expected embedded TypeScript ${EXPECTED_TYPESCRIPT}, got ${Deno.version.typescript}`,
  );
}

const sourceRoot = await Deno.realPath(sourceRootArg);
const upstreamRepoRoot = await Deno.realPath(upstreamRepoRootArg);
const consumerRepoRoot = await Deno.realPath(consumerRepoRootArg);
const expectedSourceRoot = await Deno.realPath(`${upstreamRepoRoot}/typescript`);
if (sourceRoot !== expectedSourceRoot) {
  throw new Error(
    `Source root must be the pinned repository's typescript directory: ${expectedSourceRoot}`,
  );
}

async function git(args: string[]): Promise<string> {
  const result = await new Deno.Command("git", {
    args: ["-C", upstreamRepoRoot, ...args],
    stdout: "piped",
    stderr: "piped",
  }).output();
  if (!result.success) {
    throw new Error(
      `git ${args.join(" ")} failed: ${new TextDecoder().decode(result.stderr)}`,
    );
  }
  return new TextDecoder().decode(result.stdout).trim();
}

const commit = await git(["rev-parse", "HEAD"]);
if (commit !== EXPECTED_COMMIT) {
  throw new Error(`Expected upstream commit ${EXPECTED_COMMIT}, got ${commit}`);
}
const gitStatus = await git(["status", "--porcelain=v1"]);
if (gitStatus !== "") {
  throw new Error(`Pinned upstream checkout is not clean:\n${gitStatus}`);
}

async function sha256(path: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", await Deno.readFile(path));
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

const verifiedSourceHashes: Record<string, string> = {};
for (const [relativePath, expected] of Object.entries(SOURCE_HASHES)) {
  const actual = await sha256(`${sourceRoot}/${relativePath}`);
  if (actual !== expected) {
    throw new Error(
      `Source hash mismatch for ${relativePath}: expected ${expected}, got ${actual}`,
    );
  }
  verifiedSourceHashes[`typescript/${relativePath}`] = actual;
}

const roleManifestPath =
  `${consumerRepoRoot}/algorithms/material_color/tests/fixtures/dynamic_role_manifest_2026.tsv`;
const roleManifestHash = await sha256(roleManifestPath);
if (roleManifestHash !== EXPECTED_ROLE_MANIFEST_SHA256) {
  throw new Error(
    `Original role manifest changed: expected ${EXPECTED_ROLE_MANIFEST_SHA256}, got ${roleManifestHash}`,
  );
}

const loadUpstream = (path: string) =>
  import(pathToFileURL(`${sourceRoot}/${path}`).href);
const [
  {Hct},
  {Cam16},
  {DynamicScheme},
  {Variant},
  {MaterialDynamicColors},
  {SchemeCmf},
] = await Promise.all([
  loadUpstream("hct/hct.ts"),
  loadUpstream("hct/cam16.ts"),
  loadUpstream("dynamiccolor/dynamic_scheme.ts"),
  loadUpstream("dynamiccolor/variant.ts"),
  loadUpstream("dynamiccolor/material_dynamic_colors.ts"),
  loadUpstream("scheme/scheme_cmf.ts"),
]);

const roleAccessors = Object.getOwnPropertyNames(MaterialDynamicColors.prototype)
  .filter((name) =>
    name !== "constructor" &&
    typeof MaterialDynamicColors.prototype[name] === "function"
  );
const manifestRoles = (await Deno.readTextFile(roleManifestPath))
  .trimEnd()
  .split("\n")
  .slice(1)
  .map((line) => line.split("\t")[1]);
if (
  roleAccessors.length !== 60 ||
  roleAccessors.length !== manifestRoles.length ||
  roleAccessors.some((role, index) => role !== manifestRoles[index])
) {
  throw new Error("Runtime MaterialDynamicColors role order differs from the pinned baseline manifest");
}

const VARIANTS = [
  {id: "monochrome", value: Variant.MONOCHROME},
  {id: "neutral", value: Variant.NEUTRAL},
  {id: "tonal_spot", value: Variant.TONAL_SPOT},
  {id: "vibrant", value: Variant.VIBRANT},
  {id: "expressive", value: Variant.EXPRESSIVE},
  {id: "fidelity", value: Variant.FIDELITY},
  {id: "content", value: Variant.CONTENT},
  {id: "rainbow", value: Variant.RAINBOW},
  {id: "fruit_salad", value: Variant.FRUIT_SALAD},
  {id: "cmf", value: Variant.CMF},
] as const;
const PLATFORMS = ["phone", "watch"] as const;
const MODES = [
  {name: "light", isDark: false},
  {name: "dark", isDark: true},
] as const;
const ALPHAS = [0, 1, 127, 128, 254, 255] as const;
const RGB_SEEDS = [
  {id: "black", rgb: 0x000000},
  {id: "gray", rgb: 0x808080},
  {id: "red", rgb: 0xff0000},
  {id: "blue", rgb: 0x0000ff},
  {id: "yellow", rgb: 0xffff00},
] as const;

function argb(value: number): string {
  return `0x${(value >>> 0).toString(16).padStart(8, "0").toUpperCase()}`;
}

function rgb(value: number): string {
  return (value & 0xffffff).toString(16).padStart(6, "0").toUpperCase();
}

function float(value: number): string {
  return String(value);
}

function cell(value: unknown): string {
  const result = String(value);
  if (/[\t\n\r]/.test(result)) {
    throw new Error(`TSV cell contains a control separator: ${result}`);
  }
  return result;
}

function row(values: unknown[]): string {
  return values.map(cell).join("\t");
}

function colorWithAlpha(seed: typeof RGB_SEEDS[number], alpha: number): number {
  return (((alpha & 0xff) << 24) | seed.rgb) >>> 0;
}

function alphaId(alpha: number): string {
  return alpha.toString(16).padStart(2, "0").toUpperCase();
}

const CASE_HEADER = [
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
const ROLE_HEADER = ["case_id", "role_category", "role", "argb", "tone"];
const SCIENCE_HEADER = [
  "case_id",
  "input_argb",
  "input_alpha",
  "input_red",
  "input_green",
  "input_blue",
  "hct_argb",
  "hct_alpha",
  "hct_hue",
  "hct_chroma",
  "hct_tone",
  "hct_resolved_argb",
  "cam16_hue",
  "cam16_chroma",
  "cam16_j",
  "cam16_q",
  "cam16_m",
  "cam16_s",
  "cam16_jstar",
  "cam16_astar",
  "cam16_bstar",
  "cam16_output_argb",
  "cam16_output_alpha",
  "cam16_output_rgb",
];

const caseRows: string[] = [row(CASE_HEADER)];
const roleRows: string[] = [row(ROLE_HEADER)];
const scienceRows: string[] = [row(SCIENCE_HEADER)];
let nextCaseNumber = 0;
let singleSourceCases = 0;
let cmfMultiSourceCases = 0;
const effectiveSpecs = new Map<string, Set<string>>(
  VARIANTS.map((variant) => [variant.id, new Set<string>()]),
);

function makeScheme(
  variant: typeof VARIANTS[number],
  sources: any[],
  isDark: boolean,
  platform: string,
): any {
  if (variant.id === "cmf") {
    return new SchemeCmf(sources, isDark, 0, SPEC_VERSION, platform);
  }
  return new DynamicScheme({
    sourceColorHcts: sources,
    variant: variant.value,
    contrastLevel: 0,
    isDark,
    platform,
    specVersion: SPEC_VERSION,
  });
}

function addCase(
  scenario: string,
  sourceCase: string,
  sourceKind: string,
  rawSourceArgb: number[],
  variant: typeof VARIANTS[number],
  platform: string,
  mode: typeof MODES[number],
): void {
  const sources = rawSourceArgb.map((input) => Hct.fromInt(input));
  const scheme = makeScheme(variant, sources, mode.isDark, platform);
  const caseId = `A${String(++nextCaseNumber).padStart(6, "0")}`;
  effectiveSpecs.get(variant.id)!.add(scheme.specVersion);

  caseRows.push(row([
    caseId,
    scenario,
    variant.id,
    SPEC_VERSION,
    scheme.specVersion,
    platform,
    mode.name,
    "0",
    sourceCase,
    sourceKind,
    sources.length,
    argb(sources[0].toInt()),
    sources.map((source) => argb(source.toInt())).join("|"),
    float(sources[0].hue),
    float(sources[0].chroma),
    float(sources[0].tone),
    "none",
  ]));

  for (const role of roleAccessors) {
    const accessor = scheme.colors[role];
    const dynamicColor = role === "highestSurface"
      ? accessor.call(scheme.colors, scheme)
      : accessor.call(scheme.colors);
    roleRows.push(row([
      caseId,
      [
        "primaryPaletteKeyColor",
        "secondaryPaletteKeyColor",
        "tertiaryPaletteKeyColor",
        "neutralPaletteKeyColor",
        "neutralVariantPaletteKeyColor",
        "errorPaletteKeyColor",
      ].includes(role)
        ? "palette_key"
        : "dynamic_role",
      role,
      dynamicColor === undefined ? "null" : argb(dynamicColor.getArgb(scheme)),
      dynamicColor === undefined ? "null" : float(dynamicColor.getTone(scheme)),
    ]));
  }

  if (scenario === "alpha_single_source") {
    singleSourceCases++;
  } else {
    cmfMultiSourceCases++;
  }
}

for (const seed of RGB_SEEDS) {
  for (const alpha of ALPHAS) {
    const input = colorWithAlpha(seed, alpha);
    const source = Hct.fromInt(input);
    const cam = Cam16.fromInt(input);
    const camOutput = cam.toInt();
    const hctResolved = Hct.from(source.hue, source.chroma, source.tone).toInt();
    const sourceCase = `${seed.id}_a${alphaId(alpha)}`;
    scienceRows.push(row([
      `alpha_${sourceCase}`,
      argb(input),
      (input >>> 24) & 0xff,
      (input >>> 16) & 0xff,
      (input >>> 8) & 0xff,
      input & 0xff,
      argb(source.toInt()),
      (source.toInt() >>> 24) & 0xff,
      float(source.hue),
      float(source.chroma),
      float(source.tone),
      argb(hctResolved),
      float(cam.hue),
      float(cam.chroma),
      float(cam.j),
      float(cam.q),
      float(cam.m),
      float(cam.s),
      float(cam.jstar),
      float(cam.astar),
      float(cam.bstar),
      argb(camOutput),
      (camOutput >>> 24) & 0xff,
      rgb(camOutput),
    ]));

    for (const variant of VARIANTS) {
      for (const platform of PLATFORMS) {
        for (const mode of MODES) {
          addCase(
            "alpha_single_source",
            sourceCase,
            "argb_alpha",
            [input],
            variant,
            platform,
            mode,
          );
        }
      }
    }

    const opaque = colorWithAlpha(seed, 255);
    const cmfPairs = [
      {
        id: `cmf_primary_alpha_${seed.id}_a${alphaId(alpha)}`,
        sources: [input, opaque],
      },
      {
        id: `cmf_secondary_alpha_${seed.id}_a${alphaId(alpha)}`,
        sources: [opaque, input],
      },
    ];
    for (const testCase of cmfPairs) {
      const variant = VARIANTS.find((candidate) => candidate.id === "cmf")!;
      for (const platform of PLATFORMS) {
        for (const mode of MODES) {
          addCase(
            "cmf_multi_source_alpha",
            testCase.id,
            "multi_source_argb_alpha",
            testCase.sources,
            variant,
            platform,
            mode,
          );
        }
      }
    }
  }
}

const outputDir = outputDirArg.replace(/\/+$/, "");
await Deno.mkdir(outputDir, {recursive: true});
const outputs = [
  ["alpha_cases_2026.tsv", caseRows],
  ["alpha_roles_2026.tsv", roleRows],
  ["alpha_science_2026.tsv", scienceRows],
] as const;
for (const [name, rows] of outputs) {
  await Deno.writeTextFile(`${outputDir}/${name}`, `${rows.join("\n")}\n`);
}

console.log(JSON.stringify({
  upstreamCommit: commit,
  upstreamClean: true,
  deno: Deno.version.deno,
  typescript: Deno.version.typescript,
  requestedSpec: SPEC_VERSION,
  roles: roleAccessors.length,
  seeds: RGB_SEEDS.length,
  alphaValues: ALPHAS,
  singleSourceCases,
  cmfMultiSourceCases,
  caseRows: caseRows.length - 1,
  roleRows: roleRows.length - 1,
  scienceRows: scienceRows.length - 1,
  effectiveSpecs: Object.fromEntries(
    [...effectiveSpecs].map(([variant, specs]) => [variant, [...specs].sort()]),
  ),
  verifiedSourceHashes,
  originalRoleManifestSha256: roleManifestHash,
  outputs: outputs.map(([name, rows]) => ({name, rows: rows.length - 1})),
}, null, 2));
