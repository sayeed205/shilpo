import { assertEquals } from "@std/assert";
import { describe, it } from "@std/testing/bdd";
import {
  Align,
  badge,
  buildViewTree,
  button,
  Colors,
  column,
  container,
  divider,
  grid,
  icon,
  iconButton,
  image,
  Justify,
  list,
  loadingIndicator,
  OverflowStyle,
  progress,
  row,
  slider,
  spacer,
  stack,
  style,
  text,
  textInput,
  toggle,
} from "../src/builder/index.ts";

describe("ViewTree Builder", () => {
  it("creates container with defaults and custom options", () => {
    const c = container({
      direction: { tag: "row" },
      gap: 8,
      alignItems: Align.center,
      justifyContent: Justify.spaceBetween,
      wrap: true,
      eventId: "container-click",
      style: style({
        padding: 12,
        background: Colors.surfaceContainer,
        cornerRadius: 16,
        overflow: OverflowStyle.hidden,
      }),
    });

    assertEquals(c.tag, "container");
    if (c.tag === "container") {
      assertEquals(c.val.direction, { tag: "row" });
      assertEquals(c.val.gap, 8);
      assertEquals(c.val.alignItems, "center");
      assertEquals(c.val.justifyContent, "space-between");
      assertEquals(c.val.wrap, true);
      assertEquals(c.val.eventId, "container-click");
      assertEquals(c.val.style?.padding, 12);
      assertEquals(c.val.style?.background, "surface-container");
      assertEquals(c.val.style?.cornerRadius, 16);
      assertEquals(c.val.style?.overflow, "hidden");
    }
  });

  it("creates row, column, stack, and grid helpers", () => {
    const r = row({ gap: 4 });
    const col = column({ gap: 6 });
    const stk = stack();
    const g = grid(3, { gap: 10 });

    assertEquals(r.tag === "container" && r.val.direction, { tag: "row" });
    assertEquals(col.tag === "container" && col.val.direction, { tag: "column" });
    assertEquals(stk.tag === "container" && stk.val.direction, { tag: "stack" });
    assertEquals(g.tag === "container" && g.val.direction, { tag: "grid", val: 3 });
  });

  it("creates all leaf and interactive nodes with exact values", () => {
    const t = text("Hello World", {
      fontSize: 14,
      bold: true,
      style: style({ color: Colors.primary }),
    });
    assertEquals(t, {
      tag: "text",
      val: {
        content: "Hello World",
        fontSize: 14,
        bold: true,
        style: { color: "primary" },
      },
    });

    const ic = icon("weather-sunny", { size: 24, style: style({ color: Colors.onSurface }) });
    assertEquals(ic, {
      tag: "icon",
      val: {
        name: "weather-sunny",
        size: 24,
        style: { color: "on-surface" },
      },
    });

    const img = image("assets/icon.png", { width: 48, height: 48 });
    assertEquals(img, {
      tag: "image",
      val: {
        assetPath: "assets/icon.png",
        width: 48,
        height: 48,
        style: undefined,
      },
    });

    const btn = button("Click Me", "btn-1", { style: style({ background: Colors.primary }) });
    assertEquals(btn, {
      tag: "button",
      val: {
        label: "Click Me",
        eventId: "btn-1",
        style: { background: "primary" },
      },
    });

    const ibtn = iconButton("settings", "open-settings");
    assertEquals(ibtn, {
      tag: "icon-button",
      val: {
        iconName: "settings",
        eventId: "open-settings",
        style: undefined,
      },
    });

    const tog = toggle(false, "toggle-wifi");
    assertEquals(tog, {
      tag: "toggle",
      val: {
        value: false,
        eventId: "toggle-wifi",
        style: undefined,
      },
    });

    const sld = slider(0, 0, 100, "brightness-slider");
    assertEquals(sld, {
      tag: "slider",
      val: {
        value: 0,
        min: 0,
        max: 100,
        eventId: "brightness-slider",
        style: undefined,
      },
    });

    const txtInput = textInput("", "input-search", { placeholder: "Search..." });
    assertEquals(txtInput, {
      tag: "text-input",
      val: {
        value: "",
        eventId: "input-search",
        placeholder: "Search...",
        style: undefined,
      },
    });

    const sp = spacer(16);
    assertEquals(sp, {
      tag: "spacer",
      val: { size: 16 },
    });

    const div = divider();
    assertEquals(div, {
      tag: "divider",
    });

    const bdg = badge("Active", { style: style({ color: Colors.secondary }) });
    assertEquals(bdg, {
      tag: "badge",
      val: {
        label: "Active",
        style: { color: "secondary" },
      },
    });

    const prg = progress(0.75);
    assertEquals(prg, {
      tag: "progress",
      val: {
        value: 0.75,
        style: undefined,
      },
    });

    const li = loadingIndicator({ size: 20, color: Colors.outline });
    assertEquals(li, {
      tag: "loading-indicator",
      val: {
        size: 20,
        color: "outline",
        style: undefined,
      },
    });
  });

  it("buildViewTree flattens nested hierarchy correctly", () => {
    const treeSpec = column({
      gap: 8,
      children: [
        row({
          children: [
            icon("shilpo"),
            text("Title", { bold: true }),
          ],
        }),
        list([
          text("Item 1"),
          text("Item 2"),
        ]),
        button("Submit", "submit-event"),
      ],
    });

    const tree = buildViewTree(treeSpec);

    assertEquals(tree.root, 0);
    assertEquals(tree.nodes.length, 8);

    // Root node: column (index 0)
    const rootNode = tree.nodes[0];
    assertEquals(rootNode?.tag, "container");
    if (rootNode?.tag === "container") {
      assertEquals(Array.from(rootNode.val.children), [1, 4, 7]);
    }

    // Row node (index 1)
    const rowNode = tree.nodes[1];
    assertEquals(rowNode?.tag, "container");
    if (rowNode?.tag === "container") {
      assertEquals(Array.from(rowNode.val.children), [2, 3]);
    }

    // Leaf icon (index 2)
    assertEquals(tree.nodes[2], {
      tag: "icon",
      val: { name: "shilpo", size: undefined, style: undefined },
    });

    // Leaf text (index 3)
    assertEquals(tree.nodes[3], {
      tag: "text",
      val: { content: "Title", fontSize: undefined, bold: true, style: undefined },
    });

    // List node (index 4)
    const listNode = tree.nodes[4];
    assertEquals(listNode?.tag, "list");
    if (listNode?.tag === "list") {
      assertEquals(Array.from(listNode.val.items), [5, 6]);
    }

    // List items (indices 5, 6)
    assertEquals(tree.nodes[5], {
      tag: "text",
      val: { content: "Item 1", fontSize: undefined, bold: undefined, style: undefined },
    });
    assertEquals(tree.nodes[6], {
      tag: "text",
      val: { content: "Item 2", fontSize: undefined, bold: undefined, style: undefined },
    });
  });

  it("preserves 0, false, and empty string values without coercion", () => {
    const node = column({
      gap: 0,
      children: [
        toggle(false, "tog-0"),
        slider(0, 0, 100, "slide-0"),
        text("", { fontSize: 0 }),
        spacer(0),
        progress(0),
      ],
    });

    const tree = buildViewTree(node);
    assertEquals(tree.nodes.length, 6);

    const root = tree.nodes[0];
    if (root?.tag === "container") {
      assertEquals(root.val.gap, 0);
    }

    const togNode = tree.nodes[1];
    assertEquals(togNode?.tag === "toggle" && togNode.val.value, false);

    const slideNode = tree.nodes[2];
    assertEquals(slideNode?.tag === "slider" && slideNode.val.value, 0);

    const textNode = tree.nodes[3];
    assertEquals(textNode?.tag === "text" && textNode.val.content, "");
    assertEquals(textNode?.tag === "text" && textNode.val.fontSize, 0);

    const spacerNode = tree.nodes[4];
    assertEquals(spacerNode?.tag === "spacer" && spacerNode.val.size, 0);

    const progNode = tree.nodes[5];
    assertEquals(progNode?.tag === "progress" && progNode.val.value, 0);
  });
});
