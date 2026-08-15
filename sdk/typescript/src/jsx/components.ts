import type { ViewNodeSpec } from "../builder/nodes.ts";
import {
  type BadgeProps,
  type ButtonProps,
  type ColumnProps,
  type ContainerProps,
  type DividerProps,
  FRAGMENT_TAG,
  type FragmentProps,
  type FragmentSpec,
  type GridProps,
  type IconButtonProps,
  type IconProps,
  type ImageProps,
  type ListProps,
  type LoadingIndicatorProps,
  type ProgressProps,
  type RowProps,
  type SliderProps,
  type SpacerProps,
  type StackProps,
  type TextInputProps,
  type TextProps,
  type ToggleProps,
} from "./types.ts";

export function isFragment(value: unknown): value is FragmentSpec {
  return typeof value === "object" && value !== null &&
    (value as Record<symbol, unknown>)[FRAGMENT_TAG] === true;
}

const VALID_VIEW_NODE_TAGS = new Set([
  "container",
  "text",
  "icon",
  "image",
  "button",
  "icon-button",
  "toggle",
  "slider",
  "text-input",
  "list",
  "spacer",
  "divider",
  "badge",
  "progress",
  "loading-indicator",
]);

export function isViewNodeSpec(value: unknown): value is ViewNodeSpec {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { tag?: unknown };
  return typeof candidate.tag === "string" && VALID_VIEW_NODE_TAGS.has(candidate.tag);
}

export function normalizeScalarChildren(children: unknown, componentName: string): string {
  if (children === null || children === undefined) {
    return "";
  }
  if (typeof children === "string") {
    return children;
  }
  if (typeof children === "number") {
    if (!Number.isFinite(children)) {
      throw new TypeError(`Non-finite number (${children}) passed to <${componentName}>`);
    }
    return String(children);
  }
  if (typeof children === "boolean") {
    throw new TypeError(
      `Boolean '${children}' passed to <${componentName}>. Text children must be string or number.`,
    );
  }
  if (Array.isArray(children)) {
    let result = "";
    for (const item of children) {
      if (item === null || item === undefined || item === false) {
        continue;
      }
      if (item === true) {
        throw new TypeError(`Boolean 'true' passed to <${componentName}>.`);
      }
      if (typeof item === "string") {
        result += item;
      } else if (typeof item === "number") {
        if (!Number.isFinite(item)) {
          throw new TypeError(`Non-finite number (${item}) passed to <${componentName}>`);
        }
        result += String(item);
      } else {
        throw new TypeError(
          `Invalid child value of type ${typeof item} passed to <${componentName}>. Only strings and finite numbers are permitted.`,
        );
      }
    }
    return result;
  }
  throw new TypeError(
    `Invalid child type ${typeof children} passed to <${componentName}>. Expected string or finite number.`,
  );
}

export function normalizeChildren(children: unknown, componentName: string): ViewNodeSpec[] {
  if (children === null || children === undefined || children === false) {
    return [];
  }
  if (children === true) {
    throw new TypeError(`Boolean 'true' is not a valid child in <${componentName}>`);
  }

  if (Array.isArray(children)) {
    const list: ViewNodeSpec[] = [];
    for (const item of children) {
      list.push(...normalizeChildren(item, componentName));
    }
    return list;
  }

  if (isFragment(children)) {
    return normalizeChildren(children.children, componentName);
  }

  if (isViewNodeSpec(children)) {
    return [children];
  }

  if (typeof children === "string") {
    if (children.trim() === "") {
      // Ignore formatting-only inter-element whitespace
      return [];
    }
    throw new TypeError(
      `Raw text string '${children}' cannot be a direct child of <${componentName}>. Wrap text in a <Text> component.`,
    );
  }

  if (typeof children === "number") {
    throw new TypeError(
      `Number '${children}' cannot be a direct child of <${componentName}>. Wrap text/numbers in a <Text> component.`,
    );
  }

  throw new TypeError(
    `Invalid child of type ${typeof children} passed to <${componentName}>. Expected a ViewNode component.`,
  );
}

function assertNoChildren(props: { children?: unknown }, componentName: string): void {
  if (props.children !== undefined && props.children !== null) {
    if (Array.isArray(props.children) && props.children.length === 0) {
      return;
    }
    throw new TypeError(`<${componentName}> is a leaf component and does not accept children.`);
  }
}

export function Fragment(props: FragmentProps = {}): FragmentSpec {
  return {
    [FRAGMENT_TAG]: true,
    children: props.children,
  };
}

export function Container(props: ContainerProps = {}): ViewNodeSpec {
  const children = normalizeChildren(props.children, "Container");
  return {
    tag: "container",
    val: {
      direction: props.direction ?? { tag: "column" },
      children,
      style: props.style,
      gap: props.gap,
      alignItems: props.alignItems,
      justifyContent: props.justifyContent,
      wrap: props.wrap ?? false,
      eventId: props.eventId,
    },
  };
}

export function Row(props: RowProps = {}): ViewNodeSpec {
  const children = normalizeChildren(props.children, "Row");
  return {
    tag: "container",
    val: {
      direction: { tag: "row" },
      children,
      style: props.style,
      gap: props.gap,
      alignItems: props.alignItems,
      justifyContent: props.justifyContent,
      wrap: props.wrap ?? false,
      eventId: props.eventId,
    },
  };
}

export function Column(props: ColumnProps = {}): ViewNodeSpec {
  const children = normalizeChildren(props.children, "Column");
  return {
    tag: "container",
    val: {
      direction: { tag: "column" },
      children,
      style: props.style,
      gap: props.gap,
      alignItems: props.alignItems,
      justifyContent: props.justifyContent,
      wrap: props.wrap ?? false,
      eventId: props.eventId,
    },
  };
}

export function Stack(props: StackProps = {}): ViewNodeSpec {
  const children = normalizeChildren(props.children, "Stack");
  return {
    tag: "container",
    val: {
      direction: { tag: "stack" },
      children,
      style: props.style,
      gap: props.gap,
      alignItems: props.alignItems,
      justifyContent: props.justifyContent,
      wrap: props.wrap ?? false,
      eventId: props.eventId,
    },
  };
}

export function Grid(props: GridProps): ViewNodeSpec {
  if (typeof props.columns !== "number" || !Number.isInteger(props.columns) || props.columns <= 0) {
    throw new TypeError("<Grid> requires a positive integer 'columns' prop.");
  }
  const children = normalizeChildren(props.children, "Grid");
  return {
    tag: "container",
    val: {
      direction: { tag: "grid", val: props.columns },
      children,
      style: props.style,
      gap: props.gap,
      alignItems: props.alignItems,
      justifyContent: props.justifyContent,
      wrap: props.wrap ?? false,
      eventId: props.eventId,
    },
  };
}

export function Text(props: TextProps = {}): ViewNodeSpec {
  let content = props.content;
  const children = props.children;

  if (content !== undefined && children !== undefined && children !== null && children !== "") {
    throw new TypeError("Explicit 'content' prop and children are mutually exclusive on <Text>.");
  }

  if (children !== undefined && children !== null) {
    content = normalizeScalarChildren(children, "Text");
  }

  return {
    tag: "text",
    val: {
      content: content ?? "",
      fontSize: props.fontSize,
      bold: props.bold,
      style: props.style,
    },
  };
}

export function Icon(props: IconProps): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "Icon");
  if (!props.name || typeof props.name !== "string") {
    throw new TypeError("<Icon> requires a non-empty string 'name' prop.");
  }
  return {
    tag: "icon",
    val: {
      name: props.name,
      size: props.size,
      style: props.style,
    },
  };
}

export function Image(props: ImageProps): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "Image");
  if (!props.assetPath || typeof props.assetPath !== "string") {
    throw new TypeError("<Image> requires a non-empty string 'assetPath' prop.");
  }
  return {
    tag: "image",
    val: {
      assetPath: props.assetPath,
      width: props.width,
      height: props.height,
      style: props.style,
    },
  };
}

export function Button(props: ButtonProps): ViewNodeSpec {
  let label = props.label;
  const children = props.children;

  if (label !== undefined && children !== undefined && children !== null && children !== "") {
    throw new TypeError("Explicit 'label' prop and children are mutually exclusive on <Button>.");
  }

  if (children !== undefined && children !== null) {
    label = normalizeScalarChildren(children, "Button");
  }

  if (!props.eventId || typeof props.eventId !== "string") {
    throw new TypeError("<Button> requires a non-empty string 'eventId' prop.");
  }

  return {
    tag: "button",
    val: {
      label: label ?? "",
      eventId: props.eventId,
      style: props.style,
    },
  };
}

export function IconButton(props: IconButtonProps): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "IconButton");
  if (!props.iconName || typeof props.iconName !== "string") {
    throw new TypeError("<IconButton> requires a non-empty string 'iconName' prop.");
  }
  if (!props.eventId || typeof props.eventId !== "string") {
    throw new TypeError("<IconButton> requires a non-empty string 'eventId' prop.");
  }
  return {
    tag: "icon-button",
    val: {
      iconName: props.iconName,
      eventId: props.eventId,
      style: props.style,
    },
  };
}

export function Toggle(props: ToggleProps): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "Toggle");
  if (typeof props.value !== "boolean") {
    throw new TypeError("<Toggle> requires a boolean 'value' prop.");
  }
  if (!props.eventId || typeof props.eventId !== "string") {
    throw new TypeError("<Toggle> requires a non-empty string 'eventId' prop.");
  }
  return {
    tag: "toggle",
    val: {
      value: props.value,
      eventId: props.eventId,
      style: props.style,
    },
  };
}

export function Slider(props: SliderProps): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "Slider");
  if (typeof props.value !== "number" || !Number.isFinite(props.value)) {
    throw new TypeError("<Slider> requires a finite number 'value' prop.");
  }
  if (typeof props.min !== "number" || !Number.isFinite(props.min)) {
    throw new TypeError("<Slider> requires a finite number 'min' prop.");
  }
  if (typeof props.max !== "number" || !Number.isFinite(props.max)) {
    throw new TypeError("<Slider> requires a finite number 'max' prop.");
  }
  if (!props.eventId || typeof props.eventId !== "string") {
    throw new TypeError("<Slider> requires a non-empty string 'eventId' prop.");
  }
  return {
    tag: "slider",
    val: {
      value: props.value,
      min: props.min,
      max: props.max,
      eventId: props.eventId,
      style: props.style,
    },
  };
}

export function TextInput(props: TextInputProps): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "TextInput");
  if (typeof props.value !== "string") {
    throw new TypeError("<TextInput> requires a string 'value' prop.");
  }
  if (!props.eventId || typeof props.eventId !== "string") {
    throw new TypeError("<TextInput> requires a non-empty string 'eventId' prop.");
  }
  return {
    tag: "text-input",
    val: {
      value: props.value,
      eventId: props.eventId,
      placeholder: props.placeholder,
      style: props.style,
    },
  };
}

export function List(props: ListProps = {}): ViewNodeSpec {
  const children = normalizeChildren(props.children, "List");
  return {
    tag: "list",
    val: {
      items: children,
      style: props.style,
    },
  };
}

export function Spacer(props: SpacerProps = {}): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "Spacer");
  return {
    tag: "spacer",
    val: { size: props.size },
  };
}

export function Divider(props: DividerProps = {}): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "Divider");
  return {
    tag: "divider",
  };
}

export function Badge(props: BadgeProps = {}): ViewNodeSpec {
  let label = props.label;
  const children = props.children;

  if (label !== undefined && children !== undefined && children !== null && children !== "") {
    throw new TypeError("Explicit 'label' prop and children are mutually exclusive on <Badge>.");
  }

  if (children !== undefined && children !== null) {
    label = normalizeScalarChildren(children, "Badge");
  }

  return {
    tag: "badge",
    val: {
      label: label ?? "",
      style: props.style,
    },
  };
}

export function Progress(props: ProgressProps): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "Progress");
  if (typeof props.value !== "number" || !Number.isFinite(props.value)) {
    throw new TypeError("<Progress> requires a finite number 'value' prop.");
  }
  return {
    tag: "progress",
    val: {
      value: props.value,
      style: props.style,
    },
  };
}

export function LoadingIndicator(props: LoadingIndicatorProps = {}): ViewNodeSpec {
  assertNoChildren(props as { children?: unknown }, "LoadingIndicator");
  return {
    tag: "loading-indicator",
    val: {
      size: props.size,
      color: props.color,
      style: props.style,
    },
  };
}
