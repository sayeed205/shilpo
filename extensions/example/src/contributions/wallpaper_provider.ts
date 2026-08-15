import type { WallpaperSource } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export interface ShowcaseWallpaperSpec {
  source: WallpaperSource;
  path: string;
}

export function generateWallpaper(state: ShowcaseState): ShowcaseWallpaperSpec {
  const assetName = state.mode === "active" ? "active_theme.svg" : "idle_theme.svg";
  return {
    source: "extension-asset",
    path: `assets/${assetName}`,
  };
}
