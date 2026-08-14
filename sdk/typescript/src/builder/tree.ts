import type { ViewNode, ViewTree } from "../generated/wit.ts";
import type { ViewNodeSpec } from "./nodes.ts";

export type { ViewNode, ViewTree };

/**
 * Builds a flattened, valid `ViewTree` from a root `ViewNodeSpec` tree structure.
 *
 * Flattens all nested child nodes into a linear `nodes: ViewNode[]` list with 0-indexed
 * root and child reference arrays. Preserves all zero, false, and empty values accurately.
 */
export function buildViewTree(rootSpec: ViewNodeSpec): ViewTree {
  const nodes: ViewNode[] = [];

  function addNode(spec: ViewNodeSpec): number {
    const nodeIndex = nodes.length;

    // Reserve spot for this node
    switch (spec.tag) {
      case "container": {
        const childIndices: number[] = [];
        // Temporary placeholder
        nodes.push({
          tag: "container",
          val: {
            direction: spec.val.direction ?? { tag: "column" },
            children: new Uint32Array(0),
            style: spec.val.style,
            gap: spec.val.gap,
            alignItems: spec.val.alignItems,
            justifyContent: spec.val.justifyContent,
            wrap: spec.val.wrap ?? false,
            eventId: spec.val.eventId,
          },
        });

        if (spec.val.children) {
          for (const childSpec of spec.val.children) {
            childIndices.push(addNode(childSpec));
          }
        }

        // Update with resolved child indices
        nodes[nodeIndex] = {
          tag: "container",
          val: {
            direction: spec.val.direction ?? { tag: "column" },
            children: new Uint32Array(childIndices),
            style: spec.val.style,
            gap: spec.val.gap,
            alignItems: spec.val.alignItems,
            justifyContent: spec.val.justifyContent,
            wrap: spec.val.wrap ?? false,
            eventId: spec.val.eventId,
          },
        };
        break;
      }

      case "list": {
        const itemIndices: number[] = [];
        nodes.push({
          tag: "list",
          val: {
            items: new Uint32Array(0),
            style: spec.val.style,
          },
        });

        if (spec.val.items) {
          for (const itemSpec of spec.val.items) {
            itemIndices.push(addNode(itemSpec));
          }
        }

        nodes[nodeIndex] = {
          tag: "list",
          val: {
            items: new Uint32Array(itemIndices),
            style: spec.val.style,
          },
        };
        break;
      }

      case "text":
        nodes.push({ tag: "text", val: { ...spec.val } });
        break;

      case "icon":
        nodes.push({ tag: "icon", val: { ...spec.val } });
        break;

      case "image":
        nodes.push({ tag: "image", val: { ...spec.val } });
        break;

      case "button":
        nodes.push({ tag: "button", val: { ...spec.val } });
        break;

      case "icon-button":
        nodes.push({ tag: "icon-button", val: { ...spec.val } });
        break;

      case "toggle":
        nodes.push({ tag: "toggle", val: { ...spec.val } });
        break;

      case "slider":
        nodes.push({ tag: "slider", val: { ...spec.val } });
        break;

      case "text-input":
        nodes.push({ tag: "text-input", val: { ...spec.val } });
        break;

      case "spacer":
        nodes.push({ tag: "spacer", val: { ...spec.val } });
        break;

      case "divider":
        nodes.push({ tag: "divider" });
        break;

      case "badge":
        nodes.push({ tag: "badge", val: { ...spec.val } });
        break;

      case "progress":
        nodes.push({ tag: "progress", val: { ...spec.val } });
        break;

      case "loading-indicator":
        nodes.push({ tag: "loading-indicator", val: { ...spec.val } });
        break;
    }

    return nodeIndex;
  }

  const root = addNode(rootSpec);
  return { nodes, root };
}
