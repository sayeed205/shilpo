# Shilpo Extension API Context (`shilpo-ext-api`)

## Domain Vocabulary

- **Extension ID (`ExtensionId`)**: Reverse-domain package identifier for an extension (e.g. `io.github.alice`). Pure validation and representation.
- **Contribution ID (`ContributionId`)**: Single-segment identifier naming one contribution within an extension (e.g. `clock`).
- **Canonical ID (`CanonicalId`)**: The composed, globally-unique address of a contribution (`extension/contribution`).
- **Extension Manifest (`ExtensionManifest`)**: The parsed and validated declarative manifest defining an extension's identity, metadata, contributions, subscriptions, and requested capabilities.
- **Extension Event (`ExtensionEvent`)**: Inbound events dispatched to guest extensions (e.g. lifecycle, input, shell state updates).
- **Host Effect (`HostEffect`)**: Outbound unprivileged effect requests requested by guest extensions.
- **ViewTree (`ViewTree`)**: Declarative UI tree family emitted by extensions for UI contributions.
- **Bar Menu**: A `ViewTree` supplementary surface linked to one bar-widget contribution and projected by the Shell into
  the persistent card channel. Its geometry is host-measured and host-bounded, not declared by the extension.
- **Keyboard Shortcut Contribution (`KeyboardShortcutContribution`)**: Recommended default shortcut binding targeting an action contribution within the same extension.
- **WIT Interface (`extension.wit`)**: Canonical WIT interface definition (`shilpo:extension@0.1.0`) defining the guest/host component boundary.
