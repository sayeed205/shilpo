import type {
  Alignment,
  ContainerDirection,
  Justification,
  SemanticColorToken,
  ViewStyle,
} from "../generated/wit.ts";

export type { ContainerDirection };

export interface ContainerSpec {
  direction?: ContainerDirection;
  children?: ViewNodeSpec[];
  style?: ViewStyle;
  gap?: number;
  alignItems?: Alignment;
  justifyContent?: Justification;
  wrap?: boolean;
  eventId?: string;
}

export interface ListSpec {
  items?: ViewNodeSpec[];
  style?: ViewStyle;
}

export type ViewNodeSpec =
  | { tag: "container"; val: ContainerSpec }
  | { tag: "text"; val: { content: string; fontSize?: number; bold?: boolean; style?: ViewStyle } }
  | { tag: "icon"; val: { name: string; size?: number; style?: ViewStyle } }
  | { tag: "image"; val: { assetPath: string; width?: number; height?: number; style?: ViewStyle } }
  | { tag: "button"; val: { label: string; eventId: string; style?: ViewStyle } }
  | { tag: "icon-button"; val: { iconName: string; eventId: string; style?: ViewStyle } }
  | { tag: "toggle"; val: { value: boolean; eventId: string; style?: ViewStyle } }
  | {
    tag: "slider";
    val: { value: number; min: number; max: number; eventId: string; style?: ViewStyle };
  }
  | {
    tag: "text-input";
    val: { placeholder?: string; value: string; eventId: string; style?: ViewStyle };
  }
  | { tag: "list"; val: ListSpec }
  | { tag: "spacer"; val: { size?: number } }
  | { tag: "divider" }
  | { tag: "badge"; val: { label: string; style?: ViewStyle } }
  | { tag: "progress"; val: { value: number; style?: ViewStyle } }
  | {
    tag: "loading-indicator";
    val: { size?: number; color?: SemanticColorToken; style?: ViewStyle };
  };

export interface ContainerOptions {
  direction?: ContainerDirection;
  children?: ViewNodeSpec[];
  style?: ViewStyle;
  gap?: number;
  alignItems?: Alignment;
  justifyContent?: Justification;
  wrap?: boolean;
  eventId?: string;
}

/**
 * Creates a generic container node spec.
 */
export function container(options: ContainerOptions = {}): ViewNodeSpec {
  return {
    tag: "container",
    val: {
      direction: options.direction ?? { tag: "column" },
      children: options.children ?? [],
      style: options.style,
      gap: options.gap,
      alignItems: options.alignItems,
      justifyContent: options.justifyContent,
      wrap: options.wrap ?? false,
      eventId: options.eventId,
    },
  };
}

/**
 * Creates a row container node spec.
 */
export function row(options: Omit<ContainerOptions, "direction"> = {}): ViewNodeSpec {
  return container({ ...options, direction: { tag: "row" } });
}

/**
 * Creates a column container node spec.
 */
export function column(options: Omit<ContainerOptions, "direction"> = {}): ViewNodeSpec {
  return container({ ...options, direction: { tag: "column" } });
}

/**
 * Creates a stack container node spec.
 */
export function stack(options: Omit<ContainerOptions, "direction"> = {}): ViewNodeSpec {
  return container({ ...options, direction: { tag: "stack" } });
}

/**
 * Creates a grid container node spec with a fixed number of columns.
 */
export function grid(
  columns: number,
  options: Omit<ContainerOptions, "direction"> = {},
): ViewNodeSpec {
  return container({ ...options, direction: { tag: "grid", val: columns } });
}

/**
 * Creates a text node spec.
 */
export function text(
  content: string,
  options: { fontSize?: number; bold?: boolean; style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "text",
    val: {
      content,
      fontSize: options.fontSize,
      bold: options.bold,
      style: options.style,
    },
  };
}

/**
 * Creates an icon node spec.
 */
export function icon(
  name: string,
  options: { size?: number; style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "icon",
    val: {
      name,
      size: options.size,
      style: options.style,
    },
  };
}

/**
 * Creates an image node spec.
 */
export function image(
  assetPath: string,
  options: { width?: number; height?: number; style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "image",
    val: {
      assetPath,
      width: options.width,
      height: options.height,
      style: options.style,
    },
  };
}

/**
 * Creates an interactive button node spec.
 */
export function button(
  label: string,
  eventId: string,
  options: { style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "button",
    val: {
      label,
      eventId,
      style: options.style,
    },
  };
}

/**
 * Creates an interactive icon button node spec.
 */
export function iconButton(
  iconName: string,
  eventId: string,
  options: { style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "icon-button",
    val: {
      iconName,
      eventId,
      style: options.style,
    },
  };
}

/**
 * Creates an interactive toggle switch node spec.
 */
export function toggle(
  value: boolean,
  eventId: string,
  options: { style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "toggle",
    val: {
      value,
      eventId,
      style: options.style,
    },
  };
}

/**
 * Creates an interactive slider node spec.
 */
export function slider(
  value: number,
  min: number,
  max: number,
  eventId: string,
  options: { style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "slider",
    val: {
      value,
      min,
      max,
      eventId,
      style: options.style,
    },
  };
}

/**
 * Creates an interactive text input node spec.
 */
export function textInput(
  value: string,
  eventId: string,
  options: { placeholder?: string; style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "text-input",
    val: {
      value,
      eventId,
      placeholder: options.placeholder,
      style: options.style,
    },
  };
}

/**
 * Creates a list container node spec.
 */
export function list(
  items: ViewNodeSpec[] = [],
  options: { style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "list",
    val: {
      items,
      style: options.style,
    },
  };
}

/**
 * Creates a spacer node spec.
 */
export function spacer(size?: number): ViewNodeSpec {
  return {
    tag: "spacer",
    val: { size },
  };
}

/**
 * Creates a divider node spec.
 */
export function divider(): ViewNodeSpec {
  return {
    tag: "divider",
  };
}

/**
 * Creates a badge node spec.
 */
export function badge(
  label: string,
  options: { style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "badge",
    val: {
      label,
      style: options.style,
    },
  };
}

/**
 * Creates a progress bar node spec.
 */
export function progress(
  value: number,
  options: { style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "progress",
    val: {
      value,
      style: options.style,
    },
  };
}

/**
 * Creates a loading indicator node spec.
 */
export function loadingIndicator(
  options: { size?: number; color?: SemanticColorToken; style?: ViewStyle } = {},
): ViewNodeSpec {
  return {
    tag: "loading-indicator",
    val: {
      size: options.size,
      color: options.color,
      style: options.style,
    },
  };
}
