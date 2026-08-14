import {
  buildViewTree,
  column,
  DataValue,
  defineExtension,
  row,
  text,
  toggle,
} from "../../../src/index.ts";
import { createTestHost } from "../../../src/testing/index.ts";

function assert(condition: boolean, msg: string): void {
  if (!condition) {
    throw new Error(`Assertion failed: ${msg}`);
  }
}

// 1. ViewTree Builder
const tree = buildViewTree(
  column({
    gap: 8,
    children: [
      row({ children: [text("Deno Consumer")] }),
      toggle(true, "tog-1"),
    ],
  }),
);
assert(tree.root === 0, "tree.root === 0");
assert(tree.nodes.length === 4, "tree.nodes.length === 4");

// 2. DataValue
const dv = DataValue.int(42n);
assert(DataValue.isInt(dv), "DataValue.isInt(dv)");
assert(DataValue.toJs(dv) === 42n, "DataValue.toJs(dv) === 42n");

// 3. defineExtension & FakeHost
const { facade } = createTestHost();
let activated = false;

const ext = defineExtension(
  {
    onActivate(_act) {
      activated = true;
    },
    view(_id) {
      return tree;
    },
  },
  facade,
);

ext.activate({ id: "act-deno", origin: "shell-startup", extensionId: "org.shilpo.deno" });
assert(activated, "ext.activate called");
assert(ext.view("main")?.nodes.length === 4, "ext.view returns nodes");

console.log("Deno consumer smoke test passed successfully.");
