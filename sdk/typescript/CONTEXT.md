# Shilpo TypeScript Extension SDK Context (`@shilpo/ext-sdk`)

## Domain Vocabulary

- **Extension Definition (`defineExtension`)**: High-level TypeScript adapter producing the four
  canonical guest exports (`activate`, `deactivate`, `onEvent`, `view`) with typed lifecycle hooks
  and error boundary wrapping.
- **ViewTree Builders**: Ergonomic constructors (`container`, `row`, `column`, `stack`, `grid`,
  `text`, `icon`, `image`, `button`, `iconButton`, `toggle`, `slider`, `textInput`, `list`,
  `spacer`, `divider`, `badge`, `progress`, `loadingIndicator`) that produce flattened `ViewTree`
  records.
- **DataValue Helpers (`DataValue`)**: Strongly typed constructor and conversion helpers for the WIT
  `data-value` tagged union (`none`, `bool-value`, `int-value`, `float-value`, `text-value`,
  `bytes-value`, `secret-ref`).
- **Host Facade (`HostFacade`)**: Typed capability facade wrapping host imports (`actions`,
  `clipboard`, `filesystem`, `http`, `location`, `notifications`, `state`, `secrets`, `theme`,
  `wallpaper`) without unresolved runtime imports in standard JS environments.
- **Fake Host (`FakeHost`)**: In-memory hermetic host implementation used for unit testing
  extensions without live system services.
- **Manifest Definition (`defineManifest`)**: TypeScript types and helpers aligned with
  `extension-v1.schema.json` for type-checked `extension.toml` definitions.
