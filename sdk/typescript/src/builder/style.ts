import type {
  Alignment,
  Justification,
  Overflow,
  SemanticColorToken,
  ViewStyle,
} from "../generated/wit.ts";

export type { Alignment, Justification, Overflow, SemanticColorToken, ViewStyle };

export interface ColorsMap {
  primary: SemanticColorToken;
  onPrimary: SemanticColorToken;
  secondary: SemanticColorToken;
  surface: SemanticColorToken;
  surfaceContainer: SemanticColorToken;
  onSurface: SemanticColorToken;
  onSurfaceVariant: SemanticColorToken;
  outline: SemanticColorToken;
  error: SemanticColorToken;
}

export const Colors: ColorsMap = {
  primary: "primary",
  onPrimary: "on-primary",
  secondary: "secondary",
  surface: "surface",
  surfaceContainer: "surface-container",
  onSurface: "on-surface",
  onSurfaceVariant: "on-surface-variant",
  outline: "outline",
  error: "error",
};

export interface AlignMap {
  start: Alignment;
  center: Alignment;
  end: Alignment;
  stretch: Alignment;
}

export const Align: AlignMap = {
  start: "start",
  center: "center",
  end: "end",
  stretch: "stretch",
};

export interface JustifyMap {
  start: Justification;
  center: Justification;
  end: Justification;
  spaceBetween: Justification;
  spaceAround: Justification;
}

export const Justify: JustifyMap = {
  start: "start",
  center: "center",
  end: "end",
  spaceBetween: "space-between",
  spaceAround: "space-around",
};

export interface OverflowMap {
  visible: Overflow;
  hidden: Overflow;
  scroll: Overflow;
}

export const OverflowStyle: OverflowMap = {
  visible: "visible",
  hidden: "hidden",
  scroll: "scroll",
};

/**
 * Creates a validated ViewStyle object.
 */
export function style(props: ViewStyle): ViewStyle {
  return { ...props };
}
