# Extension Testing Guide

Shilpo extensions are designed to be tested **hermetically** without requiring a running Wayland compositor, live D-Bus daemon, desktop shell, display hardware, or network access.

---

## 1. Testing TypeScript Extensions with `FakeHost`

The `@shilpo/ext-sdk/testing` module provides `createTestHost` and `FakeHost` to mock all host capability interfaces in memory:

```typescript
import { assertEquals, assertNotEquals } from "@std/assert";
import { DataValue } from "@shilpo/ext-sdk";
import { createTestHost } from "@shilpo/ext-sdk/testing";
import { createShowcaseExtension } from "../src/extension.ts";

Deno.test("Showcase extension increments click counter on input event", () => {
  const { host, facade } = createTestHost();
  const showcase = createShowcaseExtension(facade);

  // 1. Verify initial view
  const initialView = showcase.ext.view("status-bar");
  assertNotEquals(initialView, undefined);

  // 2. Dispatch simulated click event
  showcase.ext.onEvent({
    tag: "input",
    val: {
      contributionId: "status-bar",
      eventId: "btn-bar-increment",
    },
  });

  // 3. Verify state and notification
  assertEquals(showcase.store.snapshot.clicks, 1);
});
```

---

## 2. Inspecting Fake Host Interactions

`FakeHost` records all outbound host invocations:

```typescript
const { host, facade } = createTestHost();

// Notifications sent
assertEquals(host.notificationsList.length, 1);
assertEquals(host.notificationsList[0]?.title, "Expected Title");

// Clipboard content
assertEquals(host.clipboardContent, "Expected Text");

// Action invocations
assertEquals(host.actionInvocations.length, 1);
```

---

## 3. Testing Degraded Host Fallbacks

Test that your extension behaves safely when host ports are unavailable:

```typescript
// Create extension with no host ports connected
const bareShowcase = createShowcaseExtension(undefined);

// Invocations should not throw or trap
bareShowcase.ext.onEvent({
  tag: "input",
  val: { contributionId: "status-bar", eventId: "btn-bar-increment" },
});
assertEquals(bareShowcase.store.snapshot.clicks, 1);
```

---

## 4. Running the Test Suite

Run automated unit and integration tests:

```bash
# In extension directory
deno test tests/

# Or via npm test if configured
npm test
```
