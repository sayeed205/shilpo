# Extension Identity

Pure data types for identifying extensions and their contributions. Zero runtime,
zero Wasmtime/WASI (ADR-0002 precedent): this crate only parses and validates
identifiers, and is depended on by both `shilpo-ext` (the runtime) and
`shilpo-config` (configuration parsing) without dragging either into the other.

## Language

**Extension ID** (`ExtensionId`):
The identity of an extension package. Reverse-domain style: three or more
dot-separated segments, each starting lowercase-alphanumeric and containing only
lowercase letters, digits, or dashes. It is a package identity, not a display
name and not an instance.
_Avoid_: extension name, extension key, addon

**Contribution ID** (`ContributionId`):
A single-segment identifier naming one contribution inside an extension
(e.g. a bar widget, action, or settings page). Contains only lowercase letters,
digits, dashes, or underscores.
_Avoid_: widget ID, action ID

**Canonical ID** (`CanonicalId`):
The composed, globally-unique address of a contribution: `extension/contribution`.
It is the canonical form as opposed to its two parts, each of which is only
meaningful in a narrower scope. Lookups across the system are keyed on this.
_Avoid_: reference, fully-qualified ID, "contrib ID"

**ID error** (`IdError`):
The scoped parse/validation error for the three ID types. Kept in this crate so
`shilpo-config` validates IDs without importing manifest-level errors from the
runtime.
_Avoid_: manifest error, parse error

The config-serialization form (`ext:<extension>/<contribution>`) is a config
term; see `desktop/config/CONTEXT.md`.

## Invariants

- A **Contribution ID is scoped to its extension**: the same contribution name
  may exist in two different extensions, so it is not globally unique. Global
  uniqueness exists only at the **Canonical ID** level (`extension/contribution`).
  This is why lookups are keyed on the composite, never on the contribution alone.
- An **Extension ID is a package identity**: validating it says nothing about
  whether the extension is installed or loadable — that is runtime concern.
