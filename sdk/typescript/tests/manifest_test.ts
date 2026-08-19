import { assertEquals } from "@std/assert";
import { describe, it } from "@std/testing/bdd";
import { defineManifest } from "../src/manifest/index.ts";

describe("Extension Manifest", () => {
  it("defineManifest provides standard defaults and valid structure", () => {
    const manifest = defineManifest({
      id: "org.shilpo.demo",
      name: "Demo Extension",
      version: "0.1.0",
      description: "A demo extension",
      contributions: {
        bar_widgets: [
          {
            id: "widget",
            name: "Demo Widget",
          },
        ],
        wallpaper_providers: [
          {
            id: "provider",
            name: "Wallpaper Provider",
            modes: ["manual", "slideshow"],
            targets: ["global", "workspace"],
          },
        ],
        search_providers: [
          {
            id: "search",
            name: "Search Provider",
            modes: ["default"],
          },
        ],
      },
      capabilities: [
        { kind: "clipboard:read" },
        { kind: "clipboard:write" },
        { kind: "notifications:show" },
        { kind: "search:provide" },
      ],
      subscriptions: [
        { event: "theme_changed" },
        { event: "workspace_changed" },
        { event: "wallpaper_changed" },
      ],
    });

    assertEquals(manifest.schema_version, 1);
    assertEquals(manifest.api_version, "0.1.0");
    assertEquals(manifest.min_shilpo_version, "0.1.0");
    assertEquals(manifest.id, "org.shilpo.demo");
    assertEquals(manifest.contributions?.bar_widgets?.length, 1);
    assertEquals(manifest.contributions?.wallpaper_providers?.length, 1);
    assertEquals(manifest.contributions?.search_providers?.length, 1);
    assertEquals(manifest.capabilities?.length, 4);
    assertEquals(manifest.subscriptions?.length, 3);
  });
});
