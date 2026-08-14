import { assertEquals } from "@std/assert";
import { describe, it } from "@std/testing/bdd";
import { DataValue } from "../src/data.ts";

describe("DataValue Helpers", () => {
  it("creates all DataValue variants explicitly", () => {
    const none = DataValue.none();
    assertEquals(none, { tag: "none" });
    assertEquals(DataValue.isNone(none), true);

    const b = DataValue.bool(true);
    assertEquals(b, { tag: "bool-value", val: true });
    assertEquals(DataValue.isBool(b), true);

    const i = DataValue.int(42n);
    assertEquals(i, { tag: "int-value", val: 42n });
    assertEquals(DataValue.isInt(i), true);

    const iFromNum = DataValue.int(100);
    assertEquals(iFromNum, { tag: "int-value", val: 100n });

    const f = DataValue.float(3.14159);
    assertEquals(f, { tag: "float-value", val: 3.14159 });
    assertEquals(DataValue.isFloat(f), true);

    const t = DataValue.text("Shilpo");
    assertEquals(t, { tag: "text-value", val: "Shilpo" });
    assertEquals(DataValue.isText(t), true);

    const bytes = new Uint8Array([1, 2, 3, 4]);
    const bVal = DataValue.bytes(bytes);
    assertEquals(bVal, { tag: "bytes-value", val: bytes });
    assertEquals(DataValue.isBytes(bVal), true);

    const sec = DataValue.secretRef("sec-handle-123");
    assertEquals(sec, { tag: "secret-ref", val: { handle: "sec-handle-123" } });
    assertEquals(DataValue.isSecretRef(sec), true);
  });

  it("converts from JS primitives accurately", () => {
    assertEquals(DataValue.fromJs(null), { tag: "none" });
    assertEquals(DataValue.fromJs(undefined), { tag: "none" });
    assertEquals(DataValue.fromJs(false), { tag: "bool-value", val: false });
    assertEquals(DataValue.fromJs(0), { tag: "int-value", val: 0n });
    assertEquals(DataValue.fromJs(42), { tag: "int-value", val: 42n });
    assertEquals(DataValue.fromJs(123n), { tag: "int-value", val: 123n });
    assertEquals(DataValue.fromJs(2.718), { tag: "float-value", val: 2.718 });
    assertEquals(DataValue.fromJs(""), { tag: "text-value", val: "" });
    assertEquals(DataValue.fromJs("hello"), { tag: "text-value", val: "hello" });

    const u8 = new Uint8Array([10, 20]);
    assertEquals(DataValue.fromJs(u8), { tag: "bytes-value", val: u8 });

    assertEquals(DataValue.fromJs({ handle: "sec-1" }), {
      tag: "secret-ref",
      val: { handle: "sec-1" },
    });
    assertEquals(DataValue.fromJs({ a: 1 }), { tag: "text-value", val: '{"a":1}' });
  });

  it("unwraps DataValue to JS primitives accurately", () => {
    assertEquals(DataValue.toJs(DataValue.none()), null);
    assertEquals(DataValue.toJs(DataValue.bool(false)), false);
    assertEquals(DataValue.toJs(DataValue.int(0n)), 0n);
    assertEquals(DataValue.toJs(DataValue.float(1.5)), 1.5);
    assertEquals(DataValue.toJs(DataValue.text("")), "");
    assertEquals(DataValue.toJs(DataValue.bytes(new Uint8Array([5]))), new Uint8Array([5]));
    assertEquals(DataValue.toJs(DataValue.secretRef("h")), { handle: "h" });
  });
});
