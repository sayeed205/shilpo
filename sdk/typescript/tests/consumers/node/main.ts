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
      row({ children: [text("Node Consumer")] }),
      toggle(true, "tog-1"),
    ],
  }),
);
assert(tree.root === 0, "tree.root === 0");
assert(tree.nodes.length === 4, "tree.nodes.length === 4");

// 2. DataValue
const dv = DataValue.float(3.14);
assert(DataValue.isFloat(dv), "DataValue.isFloat(dv)");
assert(DataValue.toJs(dv) === 3.14, "DataValue.toJs(dv) === 3.14");

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

ext.activate({ id: "act-node", origin: "shell-startup", extensionId: "org.shilpo.node" });
assert(activated, "ext.activate called");
assert(ext.view("main")?.nodes.length === 4, "ext.view returns nodes");

console.log("Node ESM consumer smoke test passed successfully.");
