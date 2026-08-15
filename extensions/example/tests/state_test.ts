import { assertEquals } from "@std/assert";
import { ShowcaseStateStore } from "../src/state.ts";

Deno.test("Showcase StateStore - manages state transitions, updates, and bounding", () => {
  const store = new ShowcaseStateStore();

  // 1. Initial defaults
  assertEquals(store.snapshot.clicks, 0);
  assertEquals(store.snapshot.mode, "active");
  assertEquals(store.snapshot.notificationsEnabled, true);

  // 2. Click increment
  assertEquals(store.incrementClicks(), 1);
  assertEquals(store.incrementClicks(), 2);
  assertEquals(store.snapshot.clicks, 2);

  // 3. Mode toggle
  assertEquals(store.toggleMode(), "idle");
  assertEquals(store.snapshot.mode, "idle");
  assertEquals(store.snapshot.accentLabel, "Idle");

  // 4. Accent label update
  store.setAccentLabel("Custom Focus");
  assertEquals(store.snapshot.accentLabel, "Custom Focus");

  // 5. Notifications toggle
  store.setNotificationsEnabled(false);
  assertEquals(store.snapshot.notificationsEnabled, false);

  // 6. Log bounding (max 50 items)
  for (let i = 0; i < 60; i++) {
    store.appendLog(`Test log ${i}`);
  }
  assertEquals(store.snapshot.logs.length, 50);

  // 7. Reset
  store.reset();
  assertEquals(store.snapshot.clicks, 0);
  assertEquals(store.snapshot.mode, "active");
});
