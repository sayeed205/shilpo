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

const OWNERS: Record<string, [string, string]> = {
  bar_widgets: ["src/contributions/bar_widget.tsx", "tests/views_test.ts"],
  bar_menus: ["src/contributions/bar_menu.tsx", "tests/views_test.ts"],
  desktop_widgets: ["src/contributions/desktop_widget.tsx", "tests/views_test.ts"],
  settings_pages: ["src/contributions/settings_page.tsx", "tests/views_test.ts"],
  side_panels: ["src/contributions/side_panel.tsx", "tests/views_test.ts"],
  search_providers: ["src/contributions/search_provider.ts", "tests/events_test.ts"],
  actions: ["src/contributions/actions.ts", "tests/events_test.ts"],
  keyboard_shortcuts: ["src/contributions/keyboard_shortcuts.ts", "tests/events_test.ts"],
  background_tasks: ["src/contributions/background_task.ts", "tests/events_test.ts"],
  wallpaper_providers: ["src/contributions/wallpaper_provider.ts", "tests/events_test.ts"],
};

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
    const [source, test] = OWNERS[family]!;
    await Deno.stat(new URL(`../${source}`, import.meta.url));
    await Deno.stat(new URL(`../${test}`, import.meta.url));
  }
});
