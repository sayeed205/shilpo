import type {
  Alignment,
  ContainerDirection,
  Justification,
  SemanticColorToken,
  ViewStyle,
} from "../generated/wit.ts";
import type { ViewNodeSpec } from "../builder/nodes.ts";

export const FRAGMENT_TAG = Symbol.for("shilpo.fragment");

export interface FragmentSpec {
  [FRAGMENT_TAG]: true;
  children?: unknown;
}

export type ViewChild =
  | ViewNodeSpec
  | FragmentSpec
  | string
  | number
  | boolean
  | null
  | undefined
  | ViewChild[];

export type ViewChildren = ViewChild | ViewChild[];

export interface ContainerProps {
  direction?: ContainerDirection;
  style?: ViewStyle;
  gap?: number;
  alignItems?: Alignment;
  justifyContent?: Justification;
  wrap?: boolean;
  eventId?: string;
  children?: ViewChildren;
}

export interface RowProps extends Omit<ContainerProps, "direction"> {}
export interface ColumnProps extends Omit<ContainerProps, "direction"> {}
export interface StackProps extends Omit<ContainerProps, "direction"> {}

export interface GridProps extends Omit<ContainerProps, "direction"> {
  columns: number;
}

export interface TextProps {
  content?: string;
  fontSize?: number;
  bold?: boolean;
  style?: ViewStyle;
  children?: string | number | (string | number | boolean | null | undefined)[];
}

export interface IconProps {
  name: string;
  size?: number;
  style?: ViewStyle;
}

export interface ImageProps {
  assetPath: string;
  width?: number;
  height?: number;
  style?: ViewStyle;
}

export interface ButtonProps {
  label?: string;
  eventId: string;
  style?: ViewStyle;
  children?: string | number | (string | number | boolean | null | undefined)[];
}

export interface IconButtonProps {
  iconName: string;
  eventId: string;
  style?: ViewStyle;
}

export interface ToggleProps {
  value: boolean;
  eventId: string;
  style?: ViewStyle;
}

export interface SliderProps {
  value: number;
  min: number;
  max: number;
  eventId: string;
  style?: ViewStyle;
}

export interface TextInputProps {
  value: string;
  eventId: string;
  placeholder?: string;
  style?: ViewStyle;
}

export interface ListProps {
  style?: ViewStyle;
  children?: ViewChildren;
}

export interface SpacerProps {
  size?: number;
}

export type DividerProps = Record<string, never>;

export interface BadgeProps {
  label?: string;
  style?: ViewStyle;
  children?: string | number | (string | number | boolean | null | undefined)[];
}

export interface ProgressProps {
  value: number;
  style?: ViewStyle;
}

export interface LoadingIndicatorProps {
  size?: number;
  color?: SemanticColorToken;
  style?: ViewStyle;
}

export interface FragmentProps {
  children?: ViewChildren;
}

// Global and exported JSX namespace
// deno-lint-ignore no-namespace
export namespace JSX {
  export type Element = ViewNodeSpec;

  export interface ElementChildrenAttribute {
    // deno-lint-ignore ban-types
    children: {};
  }

  // deno-lint-ignore no-empty-interface
  export interface IntrinsicElements {
    // Intentionally empty. HTML/DOM elements are not supported.
  }

  export interface IntrinsicAttributes {
    key?: string | number | null | undefined;
  }
}
