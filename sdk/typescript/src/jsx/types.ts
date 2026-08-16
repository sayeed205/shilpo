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

export interface BaseProps {
  key?: string | number | null | undefined;
}

export type ViewChild =
  | ViewNodeSpec
  | FragmentSpec
  | string
  | number
  | false
  | null
  | undefined
  | ViewChild[];

export type ViewChildren = ViewChild | ViewChild[];
export type ViewElement = ViewNodeSpec | FragmentSpec | ViewElement[];

export interface ContainerProps extends BaseProps {
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

export interface TextProps extends BaseProps {
  content?: string;
  fontSize?: number;
  bold?: boolean;
  style?: ViewStyle;
  children?:
    | string
    | number
    | false
    | null
    | undefined
    | (string | number | false | null | undefined)[];
}

export interface IconProps extends BaseProps {
  name: string;
  size?: number;
  style?: ViewStyle;
}

export interface ImageProps extends BaseProps {
  assetPath: string;
  width?: number;
  height?: number;
  style?: ViewStyle;
}

export interface ButtonProps extends BaseProps {
  label?: string;
  eventId: string;
  style?: ViewStyle;
  children?:
    | string
    | number
    | false
    | null
    | undefined
    | (string | number | false | null | undefined)[];
}

export interface IconButtonProps extends BaseProps {
  iconName: string;
  eventId: string;
  style?: ViewStyle;
}

export interface ToggleProps extends BaseProps {
  value: boolean;
  eventId: string;
  style?: ViewStyle;
}

export interface SliderProps extends BaseProps {
  value: number;
  min: number;
  max: number;
  eventId: string;
  style?: ViewStyle;
}

export interface TextInputProps extends BaseProps {
  value: string;
  placeholder?: string;
  eventId: string;
  style?: ViewStyle;
}

export interface ListProps extends BaseProps {
  children?: ViewChildren;
  style?: ViewStyle;
}

export interface SpacerProps extends BaseProps {
  size?: number;
}

export interface DividerProps extends BaseProps {
  style?: ViewStyle;
}

export interface BadgeProps extends BaseProps {
  label?: string;
  color?: SemanticColorToken;
  style?: ViewStyle;
  children?:
    | string
    | number
    | false
    | null
    | undefined
    | (string | number | false | null | undefined)[];
}

export interface ProgressProps extends BaseProps {
  value: number;
  style?: ViewStyle;
}

export interface LoadingIndicatorProps extends BaseProps {
  size?: number;
  color?: SemanticColorToken;
  style?: ViewStyle;
}

export interface FragmentProps extends BaseProps {
  children?: ViewChildren;
}

// Global and exported JSX namespace
// deno-lint-ignore no-namespace
export namespace JSX {
  export type Element = ViewElement;

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
