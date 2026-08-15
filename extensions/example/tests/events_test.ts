import { assertEquals } from "@std/assert";
import { DataValue } from "@shilpo/ext-sdk";
import { createTestHost } from "@shilpo/ext-sdk/testing";
import { createShowcaseExtension } from "../src/extension.tsx";
import { searchCommands } from "../src/contributions/search_provider.ts";
import { generateWallpaper } from "../src/contributions/wallpaper_provider.ts";

Deno.test("Showcase Events - handles input, actions, shortcuts, tasks, and system events", () => {
  const { host, facade } = createTestHost();
  const showcase = createShowcaseExtension(facade);

  // 1. Input Event: Click Increment
  showcase.ext.onEvent({
    tag: "input",
    val: {
      contributionId: "status-bar",
      eventId: "btn-bar-increment",
    },
  });
  assertEquals(showcase.store.snapshot.clicks, 1);

  // 2. Input Event: Desktop Toggle
  showcase.ext.onEvent({
    tag: "input",
    val: {
      contributionId: "system-card",
      eventId: "btn-desktop-toggle",
    },
  });
  assertEquals(showcase.store.snapshot.mode, "idle");

  // 3. Input Event: Text Input Label Change
  showcase.ext.onEvent({
    tag: "input",
    val: {
      contributionId: "preferences",
      eventId: "input-label",
      value: DataValue.text("Work Focus"),
    },
  });
  assertEquals(showcase.store.snapshot.accentLabel, "Work Focus");

  // 4. Input Event: Notification Toggle Change
  showcase.ext.onEvent({
    tag: "input",
    val: {
      contributionId: "preferences",
      eventId: "tog-notifications",
      value: DataValue.bool(false),
    },
  });
  assertEquals(showcase.store.snapshot.notificationsEnabled, false);

  // 5. Action Invocation
  showcase.store.setNotificationsEnabled(true);
  showcase.handleAction("toggle-power");
  assertEquals(showcase.store.snapshot.mode, "active");

  // 6. Keyboard Shortcut
  showcase.handleShortcut("shortcut-toggle");
  assertEquals(showcase.store.snapshot.mode, "idle");

  // 7. Background Task
  showcase.handleBackgroundTask("sync-task");
  assertEquals(showcase.store.snapshot.lastSyncIso.length > 0, true);

  // 8. Palette events are inert; notifications require explicit action invocation.
  showcase.ext.onEvent({
    tag: "palette-generated",
    val: {
      accent: "#6200ee",
    },
  });
  assertEquals(host.notificationsList.length, 3);

  // 9. Manual scoped HTTPS request and completion.
  showcase.ext.onEvent({
    tag: "input",
    val: { contributionId: "status-menu", eventId: "btn-menu-refresh" },
  });
  assertEquals(host.httpRequests.length, 1);
  showcase.ext.onEvent({
    tag: "http-response",
    val: { requestId: "showcase-refresh", status: 200, body: "{}" },
  });

  // 10. Search Provider
  const searchResults = searchCommands("toggle");
  assertEquals(searchResults.length, 1);
  assertEquals(searchResults[0]?.id, "toggle-power");

  // 11. Wallpaper Provider
  const wallpaper = generateWallpaper(showcase.store.snapshot);
  assertEquals(wallpaper.source, "extension-asset");
});
