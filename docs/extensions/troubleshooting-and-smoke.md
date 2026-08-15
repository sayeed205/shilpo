# Troubleshooting & Live-Shell Smoke Testing

This guide covers common errors encountered during extension authoring, diagnostic resolution steps, and a structured manual smoke testing procedure for live desktop sessions.

---

## 1. Common Diagnostics & Troubleshooting

### `error[manifest.invalid]`
- **Cause**: An invalid reverse-DNS `id`, unsupported `schema_version`, missing required field, or unknown key in `extension.toml`.
- **Fix**: Verify against the [Manifest Reference](manifest-reference.md). Ensure `id` matches `^[a-z0-9]+(\.[a-z0-9_-]+)+$`.

### `error[wasm.invalid]: unsupported component import`
- **Cause**: The compiled WebAssembly binary imports interfaces outside `shilpo:extension` or permitted WASI subsets.
- **Fix**: Rebuild using the pinned QuickJS backend: `npx @bytecodealliance/jco componentize ... --backend qjs --backend-qjs-disable-async`.

### `error[settings.valid]: schema validation failed`
- **Cause**: Syntax or JSON schema specification error in `settings.schema.json`.
- **Fix**: Validate that `settings.schema.json` is a valid Draft 2020-12 schema and defines an object root.

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
