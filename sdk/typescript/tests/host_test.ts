import { assertEquals, assertThrows } from "@std/assert";
import { describe, it } from "@std/testing/bdd";
import { createTestHost } from "../src/testing/fake_host.ts";
import { createHostFacade, HostError } from "../src/host.ts";
import { DataValue } from "../src/data.ts";

describe("Host Capability Facades", () => {
  it("actions facade invokes registered action", () => {
    const { host, facade } = createTestHost();
    facade.actions.invoke("toggle-panel", DataValue.text("settings"));
    assertEquals(host.actionInvocations.length, 1);
    assertEquals(host.actionInvocations[0]?.actionId, "toggle-panel");
    assertEquals(host.actionInvocations[0]?.payload, { tag: "text-value", val: "settings" });
  });

  it("clipboard facade reads and writes text", () => {
    const { host, facade } = createTestHost();
    facade.clipboard.write("copied text");
    assertEquals(host.clipboardContent, "copied text");
    assertEquals(facade.clipboard.read(), "copied text");
  });

  it("filesystem facade reads and writes virtual files", () => {
    const { facade } = createTestHost();
    const bytes = new Uint8Array([1, 2, 3]);
    facade.filesystem.writeFile("data/config.bin", bytes);
    const read = facade.filesystem.readFile("data/config.bin");
    assertEquals(read, bytes);

    assertThrows(
      () => {
        facade.filesystem.readFile("missing.bin");
      },
      HostError,
      "File not found",
    );
  });

  it("http facade enqueues request and location facade returns requestId", () => {
    const { host, facade } = createTestHost();
    const reqId = facade.http.request({
      requestId: "req-1",
      url: "https://api.example.com/weather",
      method: "GET",
      headers: [["Accept", "application/json"]],
    });
    assertEquals(reqId, "req-1");
    assertEquals(host.httpRequests.length, 1);

    const locId = facade.location.read();
    assertEquals(locId, "loc-req-1");
  });

  it("notifications facade shows desktop notification", () => {
    const { host, facade } = createTestHost();
    facade.notifications.show({
      title: "Update",
      body: "Ready to install",
      icon: "system-update",
    });
    assertEquals(host.notificationsList.length, 1);
    assertEquals(host.notificationsList[0]?.title, "Update");
  });

  it("state facade supports KV, revisions, and watches", () => {
    const { host, facade } = createTestHost();

    // Initial read is empty
    const initial = facade.state.read("theme_mode");
    assertEquals(initial.value, undefined);

    // Write and convenience getters
    const m1 = facade.state.setString("theme_mode", "dark");
    assertEquals(m1.changed, true);
    assertEquals(m1.revision, 2n);
    assertEquals(facade.state.getString("theme_mode"), "dark");

    facade.state.setNumber("count", 42);
    assertEquals(facade.state.getNumber("count"), 42);

    facade.state.setBoolean("enabled", true);
    assertEquals(facade.state.getBoolean("enabled"), true);

    // Watch
    const watch = facade.state.watch("theme_mode");
    assertEquals(watch.watchId, 1n);
    assertEquals(watch.snapshot.value, { tag: "text-value", val: "dark" });

    // Update triggers state event
    facade.state.setString("theme_mode", "light");
    assertEquals(host.stateEvents.length, 1);
    assertEquals(host.stateEvents[0]?.key, "theme_mode");
    assertEquals(host.stateEvents[0]?.value, { tag: "text-value", val: "light" });

    // Delete
    const mDel = facade.state.delete("theme_mode");
    assertEquals(mDel.changed, true);
    assertEquals(facade.state.getString("theme_mode"), undefined);
  });

  it("secrets facade stores, reads, and deletes secrets", () => {
    const { facade } = createTestHost();
    const secretBytes = new Uint8Array([0xde, 0xad, 0xbe, 0xef]);
    const secRef = facade.secrets.set("api-token", secretBytes);
    assertEquals(typeof secRef.handle, "string");

    const readBack = facade.secrets.read("api-token", secRef);
    assertEquals(readBack, secretBytes);

    facade.secrets.delete("api-token", secRef);
    const afterDelete = facade.secrets.read("api-token", secRef);
    assertEquals(afterDelete, undefined);
  });

  it("theme and wallpaper facades read and mutate system state", () => {
    const { host, facade } = createTestHost();
    const theme = facade.theme.read();
    assertEquals(theme.mode, "dark");
    assertEquals(theme.accent, "#6750A4");

    facade.theme.setSourceColor("#FF5722");
    assertEquals(host.themeAccent, "#FF5722");

    const wp = facade.wallpaper.read();
    assertEquals(wp.path, "/path/to/current_wallpaper.jpg");

    facade.wallpaper.set({
      path: "/home/user/Pictures/mountain.jpg",
      source: "local-file",
    });
    assertEquals(host.wallpaperPath, "/home/user/Pictures/mountain.jpg");
  });

  it("empty facade throws backend-unavailable HostError", () => {
    const emptyFacade = createHostFacade();
    const err = assertThrows(
      () => {
        emptyFacade.clipboard.read();
      },
      HostError,
      "Host capability 'clipboard' is not available",
    );
    assertEquals(err.kind, "backend-unavailable");
  });
});
