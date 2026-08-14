export * from "../generated/manifest.ts";

import type { ExtensionManifest } from "../generated/manifest.ts";

/**
 * Validates and defines an extension manifest.
 */
export function defineManifest(manifest: ExtensionManifest): ExtensionManifest {
  return {
    schema_version: 1,
    api_version: "0.1.0",
    min_shilpo_version: "0.1.0",
    ...manifest,
  };
}
