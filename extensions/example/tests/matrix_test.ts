import { assertEquals } from "@std/assert";

const REQUIRED_CONTRIBUTION_FAMILIES = [
  "bar_widgets",
  "bar_menus",
  "desktop_widgets",
  "settings_pages",
  "side_panels",
  "search_providers",
  "actions",
  "keyboard_shortcuts",
  "background_tasks",
  "wallpaper_providers",
];

Deno.test("Showcase Coverage Matrix - validates all 10 contribution families in manifest and docs", async () => {
  const manifestText = await Deno.readTextFile(
    new URL("../extension.toml", import.meta.url),
  );
  const coverageText = await Deno.readTextFile(
    new URL("../COVERAGE.md", import.meta.url),
  );

  for (const family of REQUIRED_CONTRIBUTION_FAMILIES) {
    // 1. Must be declared in extension.toml
    const manifestHasFamily = manifestText.includes(`[[contributions.${family}]]`);
    assertEquals(
      manifestHasFamily,
      true,
      `Manifest extension.toml must declare contributions.${family}`,
    );

    // 2. Must be documented in COVERAGE.md
    const coverageHasFamily = coverageText.includes(`**\`${family}\`**`);
    assertEquals(
      coverageHasFamily,
      true,
      `Coverage matrix COVERAGE.md must document contribution family ${family}`,
    );
  }
});
