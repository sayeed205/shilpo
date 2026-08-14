# Shilpo Rust Extension SDK Context (`shilpo-ext-sdk`)

## Domain Vocabulary

- **Extension Trait (`Extension`)**: High-level Rust trait providing the four canonical guest exports
  (`activate`, `deactivate`, `on_event`, `view`) with default implementations and typed error propagation.
- **Export Macro (`export_extension!`)**: Macro registering an `Extension` implementation with the canonical
  WIT guest export boundary without boilerplate.
- **ViewTree Builders**: Ergonomic constructors (`container`, `row`, `column`, `stack`, `grid`, `text`, `icon`,
  `image`, `button`, `icon_button`, `toggle`, `slider`, `text_input`, `list`, `spacer`, `divider`, `badge`,
  `progress`, `loading_indicator`) and `StyleBuilder` (`style()`) producing flattened, canonical `ViewTree` records.
- **Declarative View Macro (`view!`)**: Declarative macro providing nested syntax for composing `ViewTree`
  hierarchies with standard Rust expressions, conditionals, and iterations.
- **DataValue Helpers (`DataValue`)**: Strongly typed constructors and `From` conversions for the WIT `data-value`
  tagged union.
- **State Helper (`State`)**: Ergonomic, typed helper over the canonical `state` host imports (`read`, `write`,
  `delete`, `watch`, `unwatch`) preserving namespacing, monotonic revisions, and atomic snapshots.
- **Raw Bindings (`raw` / `bindings`)**: Direct low-level generated WebAssembly Interface Type (WIT) bindings from
  `core/ext-api/wit/extension.wit`.
