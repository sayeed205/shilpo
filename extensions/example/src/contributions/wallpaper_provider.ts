import type { WallpaperSource } from "@shilpo/ext-sdk";
import type { ShowcaseState } from "../state.ts";

export interface ShowcaseWallpaperSpec {
  source: WallpaperSource;
  path: string;
}

export function generateWallpaper(state: ShowcaseState): ShowcaseWallpaperSpec {
  const assetName = state.mode === "active" ? "active_theme.png" : "idle_theme.png";
  return {
    source: "extension-asset",
    path: `assets/${assetName}`,
  };
}
