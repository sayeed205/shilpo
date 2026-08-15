import { assertEquals } from "@std/assert";
import { createTestHost } from "@shilpo/ext-sdk/testing";
import { createShowcaseExtension } from "../src/extension.ts";

Deno.test("Showcase Lifecycle - activates and deactivates gracefully", () => {
  const { facade } = createTestHost();
  const showcase = createShowcaseExtension(facade);

  // 1. Initial State
  assertEquals(showcase.store.snapshot.clicks, 0);
  assertEquals(showcase.store.snapshot.mode, "active");

  // 2. Activate
  showcase.ext.activate({
    id: "act-test-01",
    origin: "shell-startup",
    extensionId: "org.shilpo.example",
  });

  const stateAfterActivate = showcase.store.snapshot;
  assertEquals(
    stateAfterActivate.logs[0]?.includes("Extension activated by origin: shell-startup"),
    true,
  );

  // 3. Deactivate
  showcase.ext.deactivate("user-requested");
  const stateAfterDeactivate = showcase.store.snapshot;
  assertEquals(
    stateAfterDeactivate.logs[0]?.includes("Extension deactivated (user-requested)"),
    true,
  );
});
