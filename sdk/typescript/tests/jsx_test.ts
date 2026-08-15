import { assertEquals, assertThrows } from "@std/assert";
import { describe, it } from "@std/testing/bdd";
import { badge, button, column, defineExtension, icon, row, text } from "../src/index.ts";
import {
  Badge,
  Button,
  Column,
  Container,
  Divider,
  type DividerProps,
  Fragment,
  Grid,
  Icon,
  IconButton,
  type IconProps,
  Image,
  type ImageProps,
  List,
  LoadingIndicator,
  Progress,
  Row,
  Slider,
  type SliderProps,
  Spacer,
  type SpacerProps,
  Stack,
  Text,
  TextInput,
  type TextInputProps,
  Toggle,
  type ToggleProps,
  type ViewChild,
} from "../src/jsx/index.ts";
import { jsx, jsxs } from "../src/jsx-runtime/index.ts";
import { jsxDEV } from "../src/jsx-dev-runtime/index.ts";

describe("JSX ViewTree Components", () => {
  it("produces identical output between JSX components and builder functions", () => {
    const builderTree = row({
      gap: 12,
      alignItems: "center",
      children: [
        icon("star", { size: 24 }),
        column({
          gap: 4,
          children: [
            text("Hello Shilpo", { bold: true, fontSize: 16 }),
            badge("New"),
          ],
        }),
        button("Click Me", "btn-click"),
      ],
    });

    const jsxTree = Row({
      gap: 12,
      alignItems: "center",
      children: [
        Icon({ name: "star", size: 24 }),
        Column({
          gap: 4,
          children: [
            Text({ bold: true, fontSize: 16, children: "Hello Shilpo" }),
            Badge({ children: "New" }),
          ],
        }),
        Button({ eventId: "btn-click", children: "Click Me" }),
      ],
    });

    assertEquals(jsxTree, builderTree);
  });

  it("supports all structural containers: Container, Row, Column, Stack, Grid, List", () => {
    const c = Container({
      direction: { tag: "row" },
      gap: 8,
      children: [Text({ children: "Item" })],
    });
    assertEquals(c.tag, "container");
    if (c.tag === "container") {
      assertEquals(c.val.direction, { tag: "row" });
      assertEquals(c.val.gap, 8);
      assertEquals(c.val.children?.length, 1);
    }

    const r = Row({ gap: 4, children: [Text({ children: "R" })] });
    assertEquals(r.tag, "container");
    if (r.tag === "container") {
      assertEquals(r.val.direction, { tag: "row" });
    }

    const col = Column({ gap: 4, children: [Text({ children: "C" })] });
    assertEquals(col.tag, "container");
    if (col.tag === "container") {
      assertEquals(col.val.direction, { tag: "column" });
    }

    const st = Stack({ children: [Text({ children: "S" })] });
    assertEquals(st.tag, "container");
    if (st.tag === "container") {
      assertEquals(st.val.direction, { tag: "stack" });
    }

    const g = Grid({ columns: 3, gap: 10, children: [Text({ children: "G" })] });
    assertEquals(g.tag, "container");
    if (g.tag === "container") {
      assertEquals(g.val.direction, { tag: "grid", val: 3 });
    }

    const l = List({ children: [Text({ children: "L1" }), Text({ children: "L2" })] });
    assertEquals(l.tag, "list");
    if (l.tag === "list") {
      assertEquals(l.val.items?.length, 2);
    }
  });

  it("supports all leaf/control/media nodes: Text, Icon, Image, Button, IconButton, Toggle, Slider, TextInput, Spacer, Divider, Progress, LoadingIndicator", () => {
    const t = Text({ content: "Direct Content", fontSize: 14, bold: true });
    assertEquals(t, {
      tag: "text",
      val: { content: "Direct Content", fontSize: 14, bold: true, style: undefined },
    });

    const ic = Icon({ name: "settings", size: 20 });
    assertEquals(ic, { tag: "icon", val: { name: "settings", size: 20, style: undefined } });

    const img = Image({ assetPath: "assets/icon.png", width: 32, height: 32 });
    assertEquals(img, {
      tag: "image",
      val: { assetPath: "assets/icon.png", width: 32, height: 32, style: undefined },
    });

    const btn = Button({ label: "Go", eventId: "action-go" });
    assertEquals(btn, {
      tag: "button",
      val: { label: "Go", eventId: "action-go", style: undefined },
    });

    const ibtn = IconButton({ iconName: "play", eventId: "action-play" });
    assertEquals(ibtn, {
      tag: "icon-button",
      val: { iconName: "play", eventId: "action-play", style: undefined },
    });

    const tog = Toggle({ value: true, eventId: "toggle-mode" });
    assertEquals(tog, {
      tag: "toggle",
      val: { value: true, eventId: "toggle-mode", style: undefined },
    });

    const sld = Slider({ value: 50, min: 0, max: 100, eventId: "volume" });
    assertEquals(sld, {
      tag: "slider",
      val: { value: 50, min: 0, max: 100, eventId: "volume", style: undefined },
    });

    const txtIn = TextInput({ value: "Search", eventId: "input-search", placeholder: "Type here" });
    assertEquals(txtIn, {
      tag: "text-input",
      val: { value: "Search", eventId: "input-search", placeholder: "Type here", style: undefined },
    });

    const sp = Spacer({ size: 16 });
    assertEquals(sp, { tag: "spacer", val: { size: 16 } });

    const div = Divider({});
    assertEquals(div, { tag: "divider" });

    const bdg = Badge({ label: "Beta" });
    assertEquals(bdg, { tag: "badge", val: { label: "Beta", style: undefined } });

    const prg = Progress({ value: 0.75 });
    assertEquals(prg, { tag: "progress", val: { value: 0.75, style: undefined } });

    const li = LoadingIndicator({ size: 24, color: "primary" });
    assertEquals(li, {
      tag: "loading-indicator",
      val: { size: 24, color: "primary", style: undefined },
    });
  });

  it("supports text scalar children, array interpolation, and number children", () => {
    const scalarText = Text({ children: "Simple String" });
    assertEquals(scalarText, {
      tag: "text",
      val: { content: "Simple String", fontSize: undefined, bold: undefined, style: undefined },
    });

    const numText = Text({ children: 42 });
    assertEquals(numText, {
      tag: "text",
      val: { content: "42", fontSize: undefined, bold: undefined, style: undefined },
    });

    const interpolated = Text({ children: ["Count: ", 5, " items"] });
    assertEquals(interpolated, {
      tag: "text",
      val: { content: "Count: 5 items", fontSize: undefined, bold: undefined, style: undefined },
    });

    const buttonChild = Button({ eventId: "inc", children: ["Clicks (", 3, ")"] });
    assertEquals(buttonChild, {
      tag: "button",
      val: { label: "Clicks (3)", eventId: "inc", style: undefined },
    });

    const badgeChild = Badge({ children: ["v", 1.2] });
    assertEquals(badgeChild, { tag: "badge", val: { label: "v1.2", style: undefined } });
  });

  it("handles nested nodes, arrays, fragments, and conditional children properly", () => {
    const showExtra = false;
    const items = ["A", "B"];

    const tree = Row({
      children: [
        Text({ children: "Start" }),
        showExtra ? Text({ children: "Hidden" }) : null,
        undefined,
        false,
        Fragment({
          children: [
            Text({ children: "Frag1" }),
            Text({ children: "Frag2" }),
          ],
        }),
        items.map((it) => Text({ children: it })),
        Text({ children: "End" }),
      ],
    });

    if (tree.tag === "container") {
      assertEquals(tree.val.children?.length, 6);
      assertEquals(tree.val.children?.[0], {
        tag: "text",
        val: { content: "Start", fontSize: undefined, bold: undefined, style: undefined },
      });
      assertEquals(tree.val.children?.[1], {
        tag: "text",
        val: { content: "Frag1", fontSize: undefined, bold: undefined, style: undefined },
      });
      assertEquals(tree.val.children?.[2], {
        tag: "text",
        val: { content: "Frag2", fontSize: undefined, bold: undefined, style: undefined },
      });
      assertEquals(tree.val.children?.[3], {
        tag: "text",
        val: { content: "A", fontSize: undefined, bold: undefined, style: undefined },
      });
      assertEquals(tree.val.children?.[4], {
        tag: "text",
        val: { content: "B", fontSize: undefined, bold: undefined, style: undefined },
      });
      assertEquals(tree.val.children?.[5], {
        tag: "text",
        val: { content: "End", fontSize: undefined, bold: undefined, style: undefined },
      });
    }
  });

  it("ignores formatting-only whitespace strings between children", () => {
    const tree = Column({
      children: [
        "  \n  \t  ",
        Text({ children: "Line 1" }),
        "\n",
        Text({ children: "Line 2" }),
        " ",
      ],
    });

    if (tree.tag === "container") {
      assertEquals(tree.val.children?.length, 2);
    }
  });

  it("supports synchronous custom user components returning ViewNodes or Fragments", () => {
    function CustomCard(props: { title: string; count: number }) {
      return Column({
        gap: 6,
        children: [
          Text({ bold: true, children: props.title }),
          Text({ children: ["Count is: ", props.count] }),
        ],
      });
    }

    const card = jsx(CustomCard, { title: "Metrics", count: 10 });
    assertEquals(card, {
      tag: "container",
      val: {
        direction: { tag: "column" },
        children: [
          {
            tag: "text",
            val: { content: "Metrics", bold: true, fontSize: undefined, style: undefined },
          },
          {
            tag: "text",
            val: {
              content: "Count is: 10",
              bold: undefined,
              fontSize: undefined,
              style: undefined,
            },
          },
        ],
        style: undefined,
        gap: 6,
        alignItems: undefined,
        justifyContent: undefined,
        wrap: false,
        eventId: undefined,
      },
    });
  });

  it("semantic equivalence between jsx, jsxs, and jsxDEV", () => {
    const out1 = jsx(Row, { gap: 8, children: [jsx(Text, { children: "A" })] });
    const out2 = jsxs(Row, { gap: 8, children: [jsx(Text, { children: "A" })] });
    const out3 = jsxDEV(Row, { gap: 8, children: [jsx(Text, { children: "A" })] });

    assertEquals(out1, out2);
    assertEquals(out1, out3);
  });

  it("rejects unknown DOM elements and HTML tags with typed errors", () => {
    assertThrows(
      () => jsx("div", {}),
      TypeError,
      "HTML elements and DOM tags are not supported",
    );
    assertThrows(
      () => jsx("span", {}),
      TypeError,
      "HTML elements and DOM tags are not supported",
    );
    assertThrows(
      () => jsx("button", {}),
      TypeError,
      "HTML elements and DOM tags are not supported",
    );
  });

  it("rejects DOM callbacks like onClick with typed errors", () => {
    assertThrows(
      () => jsx(Button, { eventId: "click", onClick: () => {} }),
      TypeError,
      "DOM event callbacks like 'onClick' are not supported",
    );
    assertThrows(
      () => jsx(Row, { onMouseEnter: () => {} }),
      TypeError,
      "DOM event callbacks like 'onMouseEnter' are not supported",
    );
  });

  it("rejects children on leaf components", () => {
    assertThrows(
      () =>
        Icon({ name: "star", children: [Text({ children: "illegal" })] } as unknown as IconProps),
      TypeError,
      "is a leaf component and does not accept children",
    );
    assertThrows(
      () => Toggle({ value: true, eventId: "ev", children: "illegal" } as unknown as ToggleProps),
      TypeError,
      "is a leaf component and does not accept children",
    );
    assertThrows(
      () => Image({ assetPath: "a.png", children: "illegal" } as unknown as ImageProps),
      TypeError,
      "is a leaf component and does not accept children",
    );
    assertThrows(
      () =>
        Slider(
          {
            value: 1,
            min: 0,
            max: 10,
            eventId: "ev",
            children: "illegal",
          } as unknown as SliderProps,
        ),
      TypeError,
      "is a leaf component and does not accept children",
    );
    assertThrows(
      () =>
        TextInput({ value: "", eventId: "ev", children: "illegal" } as unknown as TextInputProps),
      TypeError,
      "is a leaf component and does not accept children",
    );
    assertThrows(
      () => Spacer({ size: 10, children: "illegal" } as unknown as SpacerProps),
      TypeError,
      "is a leaf component and does not accept children",
    );
    assertThrows(
      () => Divider({ children: "illegal" } as unknown as DividerProps),
      TypeError,
      "is a leaf component and does not accept children",
    );
  });

  it("rejects ambiguous content/label prop and children", () => {
    assertThrows(
      () => Text({ content: "Prop", children: "Child" }),
      TypeError,
      "Explicit 'content' prop and children are mutually exclusive",
    );
    assertThrows(
      () => Button({ label: "Prop", eventId: "ev", children: "Child" }),
      TypeError,
      "Explicit 'label' prop and children are mutually exclusive",
    );
    assertThrows(
      () => Badge({ label: "Prop", children: "Child" }),
      TypeError,
      "Explicit 'label' prop and children are mutually exclusive",
    );
  });

  it("rejects invalid child values: boolean true, plain objects, non-finite numbers", () => {
    assertThrows(
      () => Row({ children: [true as unknown as ViewChild] }),
      TypeError,
      "Boolean 'true' is not a valid child",
    );
    assertThrows(
      () => Row({ children: [{ random: "object" } as unknown as ViewChild] }),
      TypeError,
      "Invalid child of type object",
    );
    assertThrows(
      () => Text({ children: NaN }),
      TypeError,
      "Non-finite number",
    );
    assertThrows(
      () => Text({ children: Infinity }),
      TypeError,
      "Non-finite number",
    );
  });

  it("rejects invalid custom-component return values and async components", () => {
    function AsyncComp() {
      return Promise.resolve(Text({ children: "hi" }));
    }
    assertThrows(
      () => jsx(AsyncComp, {}),
      TypeError,
      "Async components are not supported",
    );

    function NullComp() {
      return null;
    }
    assertThrows(
      () => jsx(NullComp, {}),
      TypeError,
      "returned null",
    );

    function InvalidComp() {
      return { random: 123 };
    }
    assertThrows(
      () => jsx(InvalidComp, {}),
      TypeError,
      "returned an invalid view object",
    );
  });

  it("validates defineExtension view root single-node constraint", () => {
    const extValid = defineExtension({
      view(_cid) {
        return Row({ children: [Text({ children: "A" })] });
      },
    });
    const tree = extValid.view("widget");
    assertEquals(tree?.nodes.length, 2);

    const extSingleFrag = defineExtension({
      view(_cid) {
        return Fragment({ children: [Row({ children: [Text({ children: "A" })] })] });
      },
    });
    const treeFrag = extSingleFrag.view("widget");
    assertEquals(treeFrag?.nodes.length, 2);

    const extEmptyFrag = defineExtension({
      view(_cid) {
        return Fragment({ children: [] });
      },
    });
    assertThrows(
      () => extEmptyFrag.view("widget"),
      Error,
      "View returned an empty fragment. A view must normalize to exactly one root ViewNode.",
    );

    const extMultiRoot = defineExtension({
      view(_cid) {
        return Fragment({
          children: [
            Text({ children: "One" }),
            Text({ children: "Two" }),
          ],
        });
      },
    });
    assertThrows(
      () => extMultiRoot.view("widget"),
      Error,
      "View returned multiple root elements (2). A view must normalize to exactly one root ViewNode",
    );
  });
});
