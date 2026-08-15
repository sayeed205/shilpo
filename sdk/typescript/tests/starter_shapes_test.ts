import { assertEquals } from "@std/assert";
import { describe, it } from "@std/testing/bdd";
import {
  Alignment,
  button,
  column,
  defineExtension,
  divider,
  icon,
  row,
  spacer,
  text,
  toggle,
} from "../src/index.ts";
import type { Activation } from "../src/generated/wit.ts";

describe("TypeScript Starter Shapes", () => {
  const dummyActivation: Activation = {
    id: "act-1",
    origin: "shell-startup",
    extensionId: "org.example.test",
  };

  it("1. bar-widget starter shape", () => {
    let clicks = 0;
    const ext = defineExtension({
      onActivate(_activation, _host) {
        clicks = 0;
      },
      onInput(event, _host) {
        if (event.eventId === "increment") {
          clicks += 1;
        }
      },
      view(contributionId) {
        if (contributionId !== "widget") {
          return undefined;
        }
        return row({
          gap: 8,
          alignItems: "center" as Alignment,
          children: [
            icon("star", { size: 16 }),
            text(`My Widget: ${clicks}`, { bold: true }),
            button("+1", "increment"),
          ],
        });
      },
    });

    ext.activate(dummyActivation);
    ext.onEvent({
      tag: "input",
      val: { contributionId: "widget", eventId: "increment" },
    });
    assertEquals(clicks, 1);
    const tree = ext.view("widget");
    assertEquals(tree?.nodes.length, 4);
    assertEquals(ext.view("other"), undefined);
  });

  it("2. desktop-widget starter shape", () => {
    const ext = defineExtension({
      view(contributionId) {
        if (contributionId !== "widget") {
          return undefined;
        }
        return column({
          gap: 12,
          children: [
            row({
              gap: 8,
              alignItems: "center" as Alignment,
              children: [
                icon("dashboard", { size: 20 }),
                text("My Desktop", { bold: true, fontSize: 16 }),
              ],
            }),
            divider(),
            text("Desktop widget content"),
          ],
        });
      },
    });

    const tree = ext.view("widget");
    assertEquals(tree?.nodes.length, 6);
  });

  it("3. settings-page starter shape", () => {
    let enabled = true;
    const ext = defineExtension({
      onActivate(_activation, _host) {
        enabled = true;
      },
      onInput(event, _host) {
        if (event.eventId === "toggle-enabled") {
          enabled = !enabled;
        }
      },
      view(contributionId) {
        if (contributionId !== "settings") {
          return undefined;
        }
        return column({
          gap: 12,
          children: [
            text("My Settings", { bold: true, fontSize: 18 }),
            divider(),
            row({
              gap: 8,
              alignItems: "center" as Alignment,
              children: [
                text("Enable Feature"),
                spacer(),
                toggle(enabled, "toggle-enabled"),
              ],
            }),
          ],
        });
      },
    });

    ext.activate(dummyActivation);
    assertEquals(enabled, true);
    ext.onEvent({
      tag: "input",
      val: { contributionId: "settings", eventId: "toggle-enabled" },
    });
    assertEquals(enabled, false);
    const tree = ext.view("settings");
    assertEquals(tree?.nodes.length, 7);
  });

  it("4. side-panel starter shape", () => {
    const ext = defineExtension({
      view(contributionId) {
        if (contributionId !== "panel") {
          return undefined;
        }
        return column({
          gap: 8,
          children: [
            row({
              gap: 8,
              alignItems: "center" as Alignment,
              children: [
                icon("sidebar", { size: 18 }),
                text("My Panel", { bold: true }),
              ],
            }),
            divider(),
            text("Side panel content"),
          ],
        });
      },
    });

    const tree = ext.view("panel");
    assertEquals(tree?.nodes.length, 6);
  });

  it("5. action starter shape", () => {
    let ran = false;
    const ext = defineExtension({
      onActivate(_activation, _host) {
        ran = true;
      },
      onEvent(_event, _host) {},
    });

    ext.activate(dummyActivation);
    assertEquals(ran, true);
    assertEquals(ext.view("run"), undefined);
  });

  it("6. empty starter shape", () => {
    const ext = defineExtension({});
    ext.activate(dummyActivation);
    assertEquals(ext.view("none"), undefined);
  });
});
