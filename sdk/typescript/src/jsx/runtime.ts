import type { ViewNodeSpec } from "../builder/nodes.ts";
import { Fragment, isFragment, isViewNodeSpec } from "./components.ts";
import { FRAGMENT_TAG, type FragmentSpec } from "./types.ts";

export type { JSX } from "./types.ts";
export { Fragment } from "./components.ts";

function checkNoDomCallbacks(props: Record<string, unknown>): void {
  for (const key of Object.keys(props)) {
    if (
      key.startsWith("on") &&
      key.length > 2 &&
      key[2] === key[2]?.toUpperCase() &&
      typeof props[key] === "function"
    ) {
      throw new TypeError(
        `DOM event callbacks like '${key}' are not supported. Use the 'eventId' prop and handle events in 'onEvent' or 'onInput'.`,
      );
    }
  }
}

/**
 * Universal JSX transformation runtime entrypoint for Shilpo ViewTree.
 */
export function jsx(
  type: unknown,
  props: Record<string, unknown> = {},
  key?: unknown,
): ViewNodeSpec | FragmentSpec {
  checkNoDomCallbacks(props);

  if (key !== undefined) {
    props = { ...props, key };
  }

  if (
    type === Fragment || type === FRAGMENT_TAG ||
    (typeof type === "symbol" && type === Symbol.for("shilpo.fragment"))
  ) {
    return Fragment(props);
  }

  if (typeof type === "function") {
    const result = (type as (props: Record<string, unknown>) => unknown)(props);
    if (result && typeof result === "object" && "then" in result) {
      throw new TypeError("Async components are not supported in Shilpo JSX");
    }
    if (result === null || result === undefined || result === false) {
      throw new TypeError(
        `Component <${type.name || "Anonymous"}> returned ${
          String(result)
        }. Components must return a valid ViewNode or Fragment.`,
      );
    }
    if (isFragment(result)) {
      return result;
    }
    if (isViewNodeSpec(result)) {
      return result;
    }
    if (Array.isArray(result)) {
      return Fragment({ children: result });
    }
    throw new TypeError(
      `Component <${type.name || "Anonymous"}> returned an invalid view object.`,
    );
  }

  if (typeof type === "string") {
    throw new TypeError(
      `Invalid element type '<${type}>'. HTML elements and DOM tags are not supported; use Shilpo ViewTree components (Row, Column, Text, Button, etc.).`,
    );
  }

  throw new TypeError(
    `Invalid element type of type ${typeof type}. Expected a Shilpo ViewTree component function.`,
  );
}

export const jsxs = jsx;

export function jsxDEV(
  type: unknown,
  props: Record<string, unknown> = {},
  key?: unknown,
  _isStaticChildren?: boolean,
  _source?: unknown,
  _self?: unknown,
): ViewNodeSpec | FragmentSpec {
  return jsx(type, props, key);
}
