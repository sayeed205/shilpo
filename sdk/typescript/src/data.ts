import type { DataValue as WitDataValue, SecretRef } from "./generated/wit.ts";

export type DataValue = WitDataValue;
export type { SecretRef };

export interface DataValueFactory {
  none(): DataValue;
  bool(val: boolean): DataValue;
  int(val: bigint | number): DataValue;
  float(val: number): DataValue;
  text(val: string): DataValue;
  bytes(val: Uint8Array): DataValue;
  secretRef(handle: string): DataValue;
  isNone(dv: DataValue): boolean;
  isBool(dv: DataValue): boolean;
  isInt(dv: DataValue): boolean;
  isFloat(dv: DataValue): boolean;
  isText(dv: DataValue): boolean;
  isBytes(dv: DataValue): boolean;
  isSecretRef(dv: DataValue): boolean;
  fromJs(value: unknown): DataValue;
  toJs(dv: DataValue): unknown;
  unwrap(dv: DataValue): unknown;
}

/**
 * Ergonomic factory and conversion helpers for typed `DataValue` tagged union variants.
 */
export const DataValue: DataValueFactory = {
  none(): DataValue {
    return { tag: "none" };
  },

  bool(val: boolean): DataValue {
    return { tag: "bool-value", val };
  },

  int(val: bigint | number): DataValue {
    return { tag: "int-value", val: typeof val === "bigint" ? val : BigInt(Math.trunc(val)) };
  },

  float(val: number): DataValue {
    return { tag: "float-value", val };
  },

  text(val: string): DataValue {
    return { tag: "text-value", val };
  },

  bytes(val: Uint8Array): DataValue {
    return { tag: "bytes-value", val };
  },

  secretRef(handle: string): DataValue {
    return { tag: "secret-ref", val: { handle } };
  },

  isNone(dv: DataValue): boolean {
    return dv.tag === "none";
  },

  isBool(dv: DataValue): boolean {
    return dv.tag === "bool-value";
  },

  isInt(dv: DataValue): boolean {
    return dv.tag === "int-value";
  },

  isFloat(dv: DataValue): boolean {
    return dv.tag === "float-value";
  },

  isText(dv: DataValue): boolean {
    return dv.tag === "text-value";
  },

  isBytes(dv: DataValue): boolean {
    return dv.tag === "bytes-value";
  },

  isSecretRef(dv: DataValue): boolean {
    return dv.tag === "secret-ref";
  },

  fromJs(value: unknown): DataValue {
    if (value === null || value === undefined) {
      return DataValue.none();
    }
    if (typeof value === "boolean") {
      return DataValue.bool(value);
    }
    if (typeof value === "bigint") {
      return DataValue.int(value);
    }
    if (typeof value === "number") {
      if (Number.isInteger(value)) {
        return DataValue.int(value);
      }
      return DataValue.float(value);
    }
    if (typeof value === "string") {
      return DataValue.text(value);
    }
    if (value instanceof Uint8Array) {
      return DataValue.bytes(value);
    }
    if (
      typeof value === "object" &&
      value !== null &&
      "handle" in value &&
      typeof (value as { handle: unknown }).handle === "string"
    ) {
      return DataValue.secretRef((value as { handle: string }).handle);
    }
    return DataValue.text(JSON.stringify(value));
  },

  toJs(dv: DataValue): unknown {
    switch (dv.tag) {
      case "none":
        return null;
      case "bool-value":
        return dv.val;
      case "int-value":
        return dv.val;
      case "float-value":
        return dv.val;
      case "text-value":
        return dv.val;
      case "bytes-value":
        return dv.val;
      case "secret-ref":
        return dv.val;
    }
  },

  unwrap(dv: DataValue): unknown {
    return this.toJs(dv);
  },
};
