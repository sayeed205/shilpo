import { assertEquals, assertNotEquals } from "@std/assert";
import { createTestHost } from "@shilpo/ext-sdk/testing";
import { createShowcaseExtension } from "../src/extension.ts";

Deno.test("Showcase Views - renders all 5 UI contributions and handles unknown IDs", () => {
  const { facade } = createTestHost();
  const showcase = createShowcaseExtension(facade);

  // 1. Bar Widget
  const barWidget = showcase.ext.view("status-bar");
  assertNotEquals(barWidget, undefined);
  assertEquals(barWidget!.root, 0);
  assertEquals(barWidget!.nodes.length > 0, true);

  // 2. Bar Menu
  const barMenu = showcase.ext.view("status-menu");
  assertNotEquals(barMenu, undefined);
  assertEquals(barMenu!.root, 0);
  assertEquals(barMenu!.nodes.length > 0, true);

  // 3. Desktop Widget
  const desktopWidget = showcase.ext.view("system-card");
  assertNotEquals(desktopWidget, undefined);
  assertEquals(desktopWidget!.root, 0);
  assertEquals(desktopWidget!.nodes.length > 0, true);

  // 4. Settings Page
  const settingsPage = showcase.ext.view("preferences");
  assertNotEquals(settingsPage, undefined);
  assertEquals(settingsPage!.root, 0);
  assertEquals(settingsPage!.nodes.length > 0, true);

  // 5. Side Panel
  const sidePanel = showcase.ext.view("side-panel");
  assertNotEquals(sidePanel, undefined);
  assertEquals(sidePanel!.root, 0);
  assertEquals(sidePanel!.nodes.length > 0, true);

  // 6. Unknown Contribution ID returns undefined
  const unknownView = showcase.ext.view("non-existent-id");
  assertEquals(unknownView, undefined);
});
