# Troubleshooting & Live-Shell Smoke Testing

This guide covers common errors encountered during extension authoring, diagnostic resolution steps, and a structured manual smoke testing procedure for live desktop sessions.

---

## 1. Common Diagnostics & Troubleshooting

### `error[manifest.missing]`
- **Cause**: No `extension.toml` manifest found in the specified directory.
- **Fix**: Verify project path or run `shilpo ext new <name>` to scaffold a new extension.

### `error[manifest.syntax]`
- **Cause**: Malformed TOML syntax in `extension.toml`.
- **Fix**: Inspect the line and column in the diagnostic and correct the syntax error.

### `error[manifest.unsupported-schema]` / `error[manifest.unsupported-api-version]`
- **Cause**: `schema_version` or `api_version` does not match the supported platform version (`schema_version = 1`, `api_version = "0.1.0"`).
- **Fix**: Update the manifest to target the supported schema and API versions.

### `error[contribution.duplicate-id]` / `error[contribution.invalid-reference]`
- **Cause**: Duplicate contribution identifier within a family or a reference to a non-existent widget/action.
- **Fix**: Ensure all contribution IDs are unique within their family and references point to valid declared contributions.

### `error[capability.missing-subscription-grant]`
- **Cause**: Extension declares a subscription in `[[subscriptions]]` without declaring `events:subscribe` in `[[capabilities]]`.
- **Fix**: Add the `events:subscribe` capability with the required event names.

### `warning[capability.broad-network-scope]` / `warning[capability.broad-filesystem-scope]`
- **Cause**: Overly broad wildcard hosts (e.g. `*`) or filesystem roots (e.g. `/`).
- **Fix**: Scope permissions to explicit domain names and subdirectories to adhere to least privilege.

### `error[settings.invalid-defaults]`
- **Cause**: Property default value in `settings.schema.json` violates the schema's type constraints.
- **Fix**: Ensure all `default` values in `settings.schema.json` match their property schema definitions.

### `error[asset.invalid-png]` / `error[asset.invalid-svg]`
- **Cause**: An asset file in `assets/` has an invalid PNG header or malformed SVG structure.
- **Fix**: Verify image files and ensure SVG documents have valid root `<svg>` tags.

### `error[wasm.invalid]: unsupported component import`
- **Cause**: The compiled WebAssembly binary imports interfaces outside `shilpo:extension` or permitted WASI subsets.
- **Fix**: Rebuild using the pinned QuickJS backend: `npx @bytecodealliance/jco componentize ... --backend qjs --backend-qjs-disable-async`.

### `error[build.toolchain]: project-local, lockfile-backed JCO executable not found`
- **Cause**: `node_modules/@bytecodealliance/jco` or `package-lock.json` is missing.
- **Fix**: Run `npm install` in the extension directory and commit `package-lock.json`.

---

## 2. Live-Shell Manual Smoke Checklist

When testing extension changes in a running Shilpo desktop session:

| Check | Steps | Expected Result |
| :--- | :--- | :--- |
| **1. Top Bar Widget** | Start shell with extension enabled. Inspect top status bar. | Bar widget icon and label appear cleanly formatted in top bar. |
| **2. Interactive Click** | Click the bar widget action button. | Click counter increments; view invalidates and re-renders immediately. |
| **3. Dropdown Menu** | Click on the bar pill to open its attached menu. | Attached `bar_menu` opens beneath the widget; buttons are responsive. |
| **4. Desktop Widget** | Add the extension's desktop widget to a workspace. | Resizable canvas card appears with accurate dimensions and theme colors. |
| **5. Settings Panel** | Open Shilpo Settings > Extensions > [Extension Name]. | Schema-driven settings page renders toggles, inputs, and sliders. |
| **6. Theme Transition** | Switch between Light and Dark mode in Theme Settings. | Extension components re-render with new Material 3 color tokens. |
| **7. Command Palette** | Press `Super+Space` and search for extension action. | Action appears in search results and executes when selected. |
| **8. Keyboard Shortcut** | Press declared shortcut binding (e.g. `Super+Shift+S`). | Action triggers and notification or state update is visible. |
| **9. Orderly Shutdown** | Stop or reload the shell (`shilpo shell restart`). | Extension deactivates cleanly without memory leaks or zombie processes. |
