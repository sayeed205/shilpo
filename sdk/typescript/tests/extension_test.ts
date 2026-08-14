import { assertEquals, assertThrows } from "@std/assert";
import { describe, it } from "@std/testing/bdd";
import { defineExtension } from "../src/extension.ts";
import { createTestHost } from "../src/testing/fake_host.ts";
import { column, text } from "../src/builder/nodes.ts";
import type {
  Activation,
  ExtensionEvent,
  InputEvent,
  WallpaperRequest,
  WorkspaceEvent,
} from "../src/generated/wit.ts";

describe("Extension Lifecycle & Dispatch", () => {
  it("dispatches lifecycle and event handlers", () => {
    const { facade } = createTestHost();
    let activated = false;
    let deactivated = false;
    let inputReceived = "";
    let workspaceSeen = "";
    let wallpaperReasonSeen = "";

    const ext = defineExtension(
      {
        onActivate(act) {
          activated = true;
          assertEquals(act.extensionId, "org.test.sample");
        },
        onDeactivate(reason) {
          deactivated = true;
          assertEquals(reason, "user-requested");
        },
        onInput(event: InputEvent) {
          inputReceived = event.eventId;
        },
        onWorkspaceChanged(event: WorkspaceEvent) {
          workspaceSeen = event.workspaceId;
        },
        onWallpaperRequest(req: WallpaperRequest) {
          wallpaperReasonSeen = req.reason;
        },
        view(contributionId) {
          if (contributionId === "main_widget") {
            return column({ children: [text("Active")] });
          }
          return undefined;
        },
      },
      facade,
    );

    const act: Activation = {
      id: "act-1",
      origin: "shell-startup",
      extensionId: "org.test.sample",
    };
    ext.activate(act);
    assertEquals(activated, true);

    const inputEvt: ExtensionEvent = {
      tag: "input",
      val: {
        contributionId: "main_widget",
        eventId: "btn-pressed",
      },
    };
    ext.onEvent(inputEvt);
    assertEquals(inputReceived, "btn-pressed");

    const wsEvt: ExtensionEvent = {
      tag: "workspace-changed",
      val: {
        workspaceId: "ws-2",
      },
    };
    ext.onEvent(wsEvt);
    assertEquals(workspaceSeen, "ws-2");

    const wpEvt: ExtensionEvent = {
      tag: "wallpaper-request",
      val: {
        requestId: "wp-req-1",
        contributionId: "provider",
        reason: "user-next",
        mode: "manual",
        target: { tag: "global" },
      },
    };
    ext.onEvent(wpEvt);
    assertEquals(wallpaperReasonSeen, "user-next");

    const viewTree = ext.view("main_widget");
    assertEquals(viewTree?.root, 0);
    assertEquals(viewTree?.nodes.length, 2);

    const missingView = ext.view("non_existent");
    assertEquals(missingView, undefined);

    ext.deactivate("user-requested");
    assertEquals(deactivated, true);
  });

  it("redacts internal error paths on thrown exceptions", () => {
    const ext = defineExtension({
      onActivate() {
        throw new Error("Failed connecting to /home/alice/secret/db.sqlite");
      },
    });

    const err = assertThrows(() => {
      ext.activate({
        id: "act-err",
        origin: "user-input",
        extensionId: "org.test.err",
      });
    }) as Error;

    assertEquals(err.message.includes("/home/alice"), false);
    assertEquals(err.message.includes("<path>"), true);
  });

  it("redacts secret handles and rejects asynchronous lifecycle traps", () => {
    const ext = defineExtension({
      onActivate() {
        throw new Error("SecretRef(sec-123) handle=sec-123");
      },
    });
    const err = assertThrows(() => {
      ext.activate({ id: "act-secret", origin: "user-input", extensionId: "org.test.err" });
    }) as Error;
    assertEquals(err.message.includes("sec-123"), false);

    const asyncExt = defineExtension({
      onActivate: async () => {},
    });
    const asyncErr = assertThrows(() => {
      asyncExt.activate({ id: "act-async", origin: "user-input", extensionId: "org.test.async" });
    }) as Error;
    assertEquals(asyncErr.message.includes("synchronous WIT ABI"), true);
  });

  it("ensures multi-instance isolation without shared singletons", () => {
    const { facade: facadeA } = createTestHost();
    const { facade: facadeB } = createTestHost();

    let countA = 0;
    let countB = 0;

    const extA = defineExtension(
      {
        onActivate() {
          countA += 1;
        },
      },
      facadeA,
    );

    const extB = defineExtension(
      {
        onActivate() {
          countB += 10;
        },
      },
      facadeB,
    );

    extA.activate({ id: "1", origin: "shell-startup", extensionId: "a" });
    extB.activate({ id: "2", origin: "shell-startup", extensionId: "b" });

    assertEquals(countA, 1);
    assertEquals(countB, 10);
  });
});
