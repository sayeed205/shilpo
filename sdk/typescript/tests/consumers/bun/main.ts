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
      row({ children: [text("Bun Consumer")] }),
      toggle(true, "tog-1"),
    ],
  }),
);
assert(tree.root === 0, "tree.root === 0");
assert(tree.nodes.length === 4, "tree.nodes.length === 4");

// 2. DataValue
const dv = DataValue.text("bun-test");
assert(DataValue.isText(dv), "DataValue.isText(dv)");
assert(DataValue.toJs(dv) === "bun-test", "DataValue.toJs(dv) === 'bun-test'");

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

ext.activate({ id: "act-bun", origin: "shell-startup", extensionId: "org.shilpo.bun" });
assert(activated, "ext.activate called");
assert(ext.view("main")?.nodes.length === 4, "ext.view returns nodes");

console.log("Bun consumer smoke test passed successfully.");
