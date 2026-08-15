# Canonical Contribution Coverage Matrix

This matrix maps every public contribution family supported by Shilpo extension manifests (`schema_version = 1`) to its manifest declaration, owning implementation module, documentation section, and hermetic automated test.

| Contribution Family | Manifest Declaration | Owning Source Module | Documentation Section | Focused Hermetic Test |
| :--- | :--- | :--- | :--- | :--- |
| **`bar_widgets`** | `[[contributions.bar_widgets]] id = "status-bar"` | [`extensions/example/src/contributions/bar_widget.ts`](../../extensions/example/src/contributions/bar_widget.ts) | [Manifest Reference: Bar Widgets](manifest-reference.md#bar-widgets) | `tests/views_test.ts` (`renders bar widget`) |
| **`bar_menus`** | `[[contributions.bar_menus]] id = "status-menu"` | [`extensions/example/src/contributions/bar_menu.ts`](../../extensions/example/src/contributions/bar_menu.ts) | [Manifest Reference: Bar Menus](manifest-reference.md#bar-menus) | `tests/views_test.ts` (`renders bar menu`) |
| **`desktop_widgets`** | `[[contributions.desktop_widgets]] id = "system-card"` | [`extensions/example/src/contributions/desktop_widget.ts`](../../extensions/example/src/contributions/desktop_widget.ts) | [Manifest Reference: Desktop Widgets](manifest-reference.md#desktop-widgets) | `tests/views_test.ts` (`renders desktop widget`) |
| **`settings_pages`** | `[[contributions.settings_pages]] id = "preferences"` | [`extensions/example/src/contributions/settings_page.ts`](../../extensions/example/src/contributions/settings_page.ts) | [Manifest Reference: Settings Pages](manifest-reference.md#settings-pages) | `tests/views_test.ts` (`renders settings page`) |
| **`side_panels`** | `[[contributions.side_panels]] id = "side-panel"` | [`extensions/example/src/contributions/side_panel.ts`](../../extensions/example/src/contributions/side_panel.ts) | [Manifest Reference: Side Panels](manifest-reference.md#side-panels) | `tests/views_test.ts` (`renders side panel`) |
| **`search_providers`** | `[[contributions.search_providers]] id = "search-commands"` | [`extensions/example/src/contributions/search_provider.ts`](../../extensions/example/src/contributions/search_provider.ts) | [Manifest Reference: Search Providers](manifest-reference.md#search-providers) | `tests/events_test.ts` (`queries search provider`) |
| **`actions`** | `[[contributions.actions]] id = "toggle-power"` | [`extensions/example/src/contributions/actions.ts`](../../extensions/example/src/contributions/actions.ts) | [Manifest Reference: Actions](manifest-reference.md#actions) | `tests/events_test.ts` (`handles action invocation`) |
| **`keyboard_shortcuts`** | `[[contributions.keyboard_shortcuts]] id = "shortcut-toggle"` | [`extensions/example/src/contributions/keyboard_shortcuts.ts`](../../extensions/example/src/contributions/keyboard_shortcuts.ts) | [Manifest Reference: Keyboard Shortcuts](manifest-reference.md#keyboard-shortcuts) | `tests/events_test.ts` (`triggers keyboard shortcut`) |
| **`background_tasks`** | `[[contributions.background_tasks]] id = "sync-task"` | [`extensions/example/src/contributions/background_task.ts`](../../extensions/example/src/contributions/background_task.ts) | [Manifest Reference: Background Tasks](manifest-reference.md#background-tasks) | `tests/events_test.ts` (`executes background sync task`) |
| **`wallpaper_providers`** | `[[contributions.wallpaper_providers]] id = "solid-wallpapers"` | [`extensions/example/src/contributions/wallpaper_provider.ts`](../../extensions/example/src/contributions/wallpaper_provider.ts) | [Manifest Reference: Wallpaper Providers](manifest-reference.md#wallpaper-providers) | `tests/events_test.ts` (`generates solid wallpaper`) |

## Mechanical Invariant Verification

This matrix is mechanically validated by:

1. **TypeScript Conformance Suite** (`extensions/example/tests/matrix_test.ts`): Parses `extension.toml` and verifies that every contribution family is represented with matching source files and test cases.
2. **Rust Invariant Test Suite** (`desktop/shilpo/tests/examples_verification.rs`): Deserializes `extension.toml` as `shilpo_ext_api::ExtensionManifest` and verifies that all 10 contribution variants are present in `Contributions`.
