# Shilpo Extension Contribution Coverage Matrix

This matrix maps every public contribution family supported by Shilpo extension manifests
(`schema_version = 1`) to its manifest declaration, owning implementation module, documentation
section, and hermetic automated test.

| Contribution Family       | Manifest Declaration                                            | Owning Source Module                                                                 | Documentation Section                                                                                      | Focused Hermetic Test                                    |
| :------------------------ | :-------------------------------------------------------------- | :----------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------- | :------------------------------------------------------- |
| **`bar_widgets`**         | `[[contributions.bar_widgets]] id = "status-bar"`               | [`src/contributions/bar_widget.ts`](src/contributions/bar_widget.ts)                 | [Manifest Reference: Bar Widgets](../../docs/extensions/manifest-reference.md#bar-widgets)                 | `tests/views_test.ts` (`renders bar widget`)             |
| **`bar_menus`**           | `[[contributions.bar_menus]] id = "status-menu"`                | [`src/contributions/bar_menu.ts`](src/contributions/bar_menu.ts)                     | [Manifest Reference: Bar Menus](../../docs/extensions/manifest-reference.md#bar-menus)                     | `tests/views_test.ts` (`renders bar menu`)               |
| **`desktop_widgets`**     | `[[contributions.desktop_widgets]] id = "system-card"`          | [`src/contributions/desktop_widget.ts`](src/contributions/desktop_widget.ts)         | [Manifest Reference: Desktop Widgets](../../docs/extensions/manifest-reference.md#desktop-widgets)         | `tests/views_test.ts` (`renders desktop widget`)         |
| **`settings_pages`**      | `[[contributions.settings_pages]] id = "preferences"`           | [`src/contributions/settings_page.ts`](src/contributions/settings_page.ts)           | [Manifest Reference: Settings Pages](../../docs/extensions/manifest-reference.md#settings-pages)           | `tests/views_test.ts` (`renders settings page`)          |
| **`side_panels`**         | `[[contributions.side_panels]] id = "side-panel"`               | [`src/contributions/side_panel.ts`](src/contributions/side_panel.ts)                 | [Manifest Reference: Side Panels](../../docs/extensions/manifest-reference.md#side-panels)                 | `tests/views_test.ts` (`renders side panel`)             |
| **`search_providers`**    | `[[contributions.search_providers]] id = "search-commands"`     | [`src/contributions/search_provider.ts`](src/contributions/search_provider.ts)       | [Manifest Reference: Search Providers](../../docs/extensions/manifest-reference.md#search-providers)       | `tests/events_test.ts` (`queries search provider`)       |
| **`actions`**             | `[[contributions.actions]] id = "toggle-power"`                 | [`src/contributions/actions.ts`](src/contributions/actions.ts)                       | [Manifest Reference: Actions](../../docs/extensions/manifest-reference.md#actions)                         | `tests/events_test.ts` (`handles action invocation`)     |
| **`keyboard_shortcuts`**  | `[[contributions.keyboard_shortcuts]] id = "shortcut-toggle"`   | [`src/contributions/keyboard_shortcuts.ts`](src/contributions/keyboard_shortcuts.ts) | [Manifest Reference: Keyboard Shortcuts](../../docs/extensions/manifest-reference.md#keyboard-shortcuts)   | `tests/events_test.ts` (`triggers keyboard shortcut`)    |
| **`background_tasks`**    | `[[contributions.background_tasks]] id = "sync-task"`           | [`src/contributions/background_task.ts`](src/contributions/background_task.ts)       | [Manifest Reference: Background Tasks](../../docs/extensions/manifest-reference.md#background-tasks)       | `tests/events_test.ts` (`executes background sync task`) |
| **`wallpaper_providers`** | `[[contributions.wallpaper_providers]] id = "solid-wallpapers"` | [`src/contributions/wallpaper_provider.ts`](src/contributions/wallpaper_provider.ts) | [Manifest Reference: Wallpaper Providers](../../docs/extensions/manifest-reference.md#wallpaper-providers) | `tests/events_test.ts` (`generates solid wallpaper`)     |

## Mechanical Invariant Verification

This matrix is validated by:

1. **TypeScript Conformance Suite** (`extensions/example/tests/matrix_test.ts`): Parses
   `extension.toml` and verifies that every contribution family is represented with matching source
   files and test cases.
2. **Rust Schema Invariant Suite** (`desktop/shilpo/tests/examples_verification.rs`): Deserializes
   `extension.toml` as `shilpo_ext_api::ExtensionManifest` and verifies that all 10 contribution
   variants are present in `Contributions`.
