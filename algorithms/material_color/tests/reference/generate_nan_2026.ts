/*
 * Focused NaN contrast regression vectors from the pinned official TypeScript.
 * The generator calls upstream DynamicScheme/MaterialDynamicColors directly;
 * no color calculation is reimplemented here.
 */

import {pathToFileURL} from "node:url";

const [sourceRootArg, outputPath] = Deno.args;
if (!sourceRootArg || !outputPath) {
  throw new Error(
    "usage: generate_nan_2026.ts <upstream-typescript-dir> <output-tsv>",
  );
}

const sourceRoot = sourceRootArg.replace(/\/+$/, "");
const loadUpstream = (path: string) =>
  import(pathToFileURL(`${sourceRoot}/${path}`).href);

const [
  {Hct},
  {Contrast},
  {DynamicScheme},
  {Variant},
  {MaterialDynamicColors},
] = await Promise.all([
  loadUpstream("hct/hct.ts"),
  loadUpstream("contrast/contrast.ts"),
  loadUpstream("dynamiccolor/dynamic_scheme.ts"),
  loadUpstream("dynamiccolor/variant.ts"),
  loadUpstream("dynamiccolor/material_dynamic_colors.ts"),
]);

const variants = [
  {name: "neutral", value: Variant.NEUTRAL},
  {name: "tonal_spot", value: Variant.TONAL_SPOT},
  {name: "vibrant", value: Variant.VIBRANT},
  {name: "expressive", value: Variant.EXPRESSIVE},
] as const;
const seeds = [
  0xff6750a4,
  0xff0000ff,
  0xffff0000,
  0xff00ff00,
  0xffffff00,
  0xff008577,
  0xff004d40,
  0xfff9de6c,
];
const roleAccessors = Object.getOwnPropertyNames(
  MaterialDynamicColors.prototype,
).filter((name) =>
  name !== "constructor" &&
  typeof MaterialDynamicColors.prototype[name] === "function"
);

function argb(value: number): string {
  return `0x${(value >>> 0).toString(16).padStart(8, "0").toUpperCase()}`;
}

function cell(value: unknown): string {
  const result = String(value);
  if (result.includes("\t") || result.includes("\n") || result.includes("\r")) {
    throw new Error(`TSV cell contains a control separator: ${result}`);
  }
  return result;
}

function row(values: unknown[]): string {
  return values.map(cell).join("\t");
}

const output = [row([
  "case_id",
  "variant",
  "effective_spec",
  "platform",
  "mode",
  "contrast_level",
  "source_argb",
  "role",
  "branch",
  "base_tone",
  "background_tone",
  "required_ratio",
  "canonical_tone",
  "canonical_argb",
])];
let noPairCount = 0;
let pairCount = 0;
let qualifyingChangedBaseToneCount = 0;
const coveredVariants = new Set<string>();

for (const variant of variants) {
  for (const isDark of [false, true]) {
    for (const platform of ["phone", "watch"] as const) {
      const mode = isDark ? "dark" : "light";
      const candidates = new Map<string, Array<{
        role: string;
        sourceArgb: number;
        baseTone: number;
        backgroundTone: number;
        requiredRatio: number;
        canonicalTone: number;
        canonicalArgb: number;
        margin: number;
      }>>([
        ["single", []],
        ["tone_delta_pair", []],
      ]);

      // Try a small fixed seed catalog so all four 2025-effective variants can
      // be represented without relying on a specially tuned single source.
      for (const sourceArgb of seeds) {
        const scheme = new DynamicScheme({
          sourceColorHct: Hct.fromInt(sourceArgb),
          variant: variant.value,
          contrastLevel: Number.NaN,
          isDark,
          platform,
          specVersion: "2026",
        });
        if (scheme.specVersion !== "2025") {
          throw new Error(`${variant.name} unexpectedly resolved to ${scheme.specVersion}`);
        }

        const colors = scheme.colors;
        for (const roleName of roleAccessors) {
          const accessor = (MaterialDynamicColors.prototype as any)[roleName];
          const color = roleName === "highestSurface"
            ? accessor.call(colors, scheme)
            : accessor.call(colors);
          if (!color || !color.background || !color.contrastCurve) continue;

          const pair = color.toneDeltaPair?.(scheme);
          const branch = pair ? "tone_delta_pair" : "single";
          const background = color.background(scheme);
          const curve = color.contrastCurve(scheme);
          if (!background || !curve) continue;

          const baseTone = color.tone(scheme);
          const backgroundTone = background.getTone(scheme);
          const requiredRatio = curve.get(scheme.contrastLevel);
          const baseRatio = Contrast.ratioOfTones(backgroundTone, baseTone);
          if (!(baseRatio >= requiredRatio)) continue;

          const canonicalTone = color.getTone(scheme);
          if (canonicalTone === baseTone) continue;
          const canonicalArgb = color.getArgb(scheme);
          candidates.get(branch)!.push({
            role: roleName,
            sourceArgb,
            baseTone,
            backgroundTone,
            requiredRatio,
            canonicalTone,
            canonicalArgb,
            margin: baseRatio - requiredRatio,
          });
        }
      }

      // Prefer a clear contrast-satisfied base tone that canonical NaN handling
      // still recalculates. Emit each branch when upstream exposes a candidate.
      for (const branch of ["single", "tone_delta_pair"] as const) {
        const choices = candidates.get(branch)!;
        choices.sort((left, right) => right.margin - left.margin);
        const selected = choices[0];
        if (!selected) continue;
        const index = output.length;
        output.push(row([
          `N${String(index).padStart(3, "0")}`,
          variant.name,
          "2025",
          platform,
          mode,
          "NaN",
          argb(selected.sourceArgb),
          selected.role,
          branch,
          selected.baseTone,
          selected.backgroundTone,
          selected.requiredRatio,
          selected.canonicalTone,
          argb(selected.canonicalArgb),
        ]));
        coveredVariants.add(variant.name);
        qualifyingChangedBaseToneCount++;
        if (branch === "single") noPairCount++;
        else pairCount++;
      }
    }
  }
}

if (coveredVariants.size !== variants.length || noPairCount === 0 || pairCount === 0) {
  throw new Error(
    `insufficient NaN candidates: ${noPairCount} no-pair, ${pairCount} pair, variants ${[...coveredVariants]}`,
  );
}

await Deno.writeTextFile(outputPath, `${output.join("\n")}\n`);
console.log(JSON.stringify({
  sourceRoot,
  requestedSpec: "2026",
  effectiveSpec: "2025",
  variants: variants.map(({name}) => name),
  platforms: ["phone", "watch"],
  modes: ["light", "dark"],
  contrastLevel: "NaN",
  cases: output.length - 1,
  noPairCases: noPairCount,
  toneDeltaPairCases: pairCount,
  qualifyingChangedBaseToneCases: qualifyingChangedBaseToneCount,
  outputPath,
}, null, 2));
