# Shilpo Showcase Extension (`org.shilpo.example`)

> **The Single Canonical TypeScript Showcase for the Shilpo Extension Ecosystem.**

This extension is the authoritative reference implementation demonstrating how to build, structure,
test, and package an extension targeting the Shilpo Linux desktop environment using the published
`@shilpo/ext-sdk` TypeScript SDK.

---

## Features & Contribution Families

This showcase demonstrates all **ten** Shilpo extension contribution families:

1. **Bar Widget (`status-bar`)**: Compact status pill on the system top bar with live state and
   interactive click handler.
2. **Bar Menu (`status-menu`)**: Dropdown popup attached to the bar widget containing status details
   and action buttons.
3. **Desktop Widget (`system-card`)**: Resizable desktop canvas card featuring progress
   visualization, theme integration, and controls.
4. **Settings Page (`preferences`)**: Declarative configuration page driven by
   [`settings.schema.json`](settings.schema.json).
5. **Side Panel (`side-panel`)**: Full-height dockable side panel showing extension diagnostics and
   recent activity logs.
6. **Search Provider (`search-commands`)**: Integrated search provider returning executable quick
   actions.
7. **Action (`toggle-power`)**: Executable command registered in the command palette.
8. **Keyboard Shortcut (`shortcut-toggle`)**: Default hotkey binding (`Super+Shift+S`) bound to
   `toggle-power`.
9. **Background Task (`sync-task`)**: Periodic background maintenance task performing data
   synchronization.
10. **Wallpaper Provider (`solid-wallpapers`)**: Dynamic wallpaper background provider for global
    and workspace surfaces.

See the complete [Contribution Coverage Matrix](COVERAGE.md) for module mappings.

---

## Security & Capability Minimization

In adherence to Shilpo's least-privilege architecture:

- **No Wildcard Scopes**: Every capability declared in [`extension.toml`](extension.toml) specifies
  an explicit, narrow scope (`notifications:show`, `theme:read`, `clipboard:read`,
  `clipboard:write`, `wallpaper:read`, `events:subscribe`).
- **No Disruptive Behavior**: Disruptive actions (notifications, clipboard writes) occur only after
  explicit user interaction (e.g. clicking a menu item or invoking an action), never unprovoked on
  startup.
- **Degraded Execution**: When host APIs (state, clipboard, notifications) are absent or fail, all
  features fall back gracefully to bounded in-memory representations without crashing.

---

## Development & Build

### Prerequisites

- Node.js (v18+) with npm, or Deno
- Shilpo CLI (`shilpo`)

### 1. Install Dependencies

```bash
npm ci
```

### 2. Build WebAssembly Component

Compile the TypeScript source into a WebAssembly component using the pinned QuickJS backend:

```bash
shilpo ext build
```

Or manually using `jco`:

```bash
node_modules/.bin/jco componentize src/extension.ts \
  --wit node_modules/@shilpo/ext-sdk/wit \
  --world-name extension \
  --backend qjs \
  --backend-qjs-disable-async \
  -o extension.wasm
```

### 3. Lint Manifest, Capabilities & Schemas

Perform ahead-of-time linting on the manifest, capability declarations, and settings schema:

```bash
shilpo ext lint extensions/example
```

### 4. Validate Component & Manifest

Validate the manifest, settings schema, and component interface:

```bash
shilpo ext check extensions/example
```

### 5. Package Extension

Bundle into a signed or unsigned distribution archive (`.shilpo-ext`):

```bash
shilpo ext pack extensions/example
```

### 6. Run Automated Tests

Run the hermetic test suite exercising lifecycle, views, events, state, and coverage invariants:

```bash
deno test tests/
```

---

## Manual Live-Shell Smoke Test Checklist

When testing inside a live Shilpo shell session:

- [ ] **Top Bar**: Confirm `Showcase: Active (0)` appears in the top bar. Click "Click" and verify
      counter increments to `(1)`.
- [ ] **Bar Menu**: Click the bar widget to open `Showcase Menu`. Verify clicks, last sync
      timestamp, and click "Toggle Mode" to switch to `Idle`.
- [ ] **Desktop Widget**: Open Desktop Widgets and add `Showcase Desktop Card`. Verify milestone
      progress bar and increment button.
- [ ] **Settings**: Open Shilpo Settings > Extensions > Shilpo Showcase Extension. Verify toggles
      and text input update state.
- [ ] **Side Panel**: Open Side Panel and verify recent log items appear in real time.
- [ ] **Command Palette**: Press `Super+Space`, type `Showcase: Toggle Mode`, and verify action
      triggers.
- [ ] **Keyboard Shortcut**: Press `Super+Shift+S` and verify mode toggles with a desktop
      notification.

---

## Documentation

- [Shilpo Extension Documentation Hub](../../docs/extensions/index.md)
- [Manifest & Schema Reference](../../docs/extensions/manifest-reference.md)
- [Architecture & Lifecycle Guide](../../docs/extensions/architecture-and-lifecycle.md)
- [Security & Capabilities](../../docs/extensions/security-and-capabilities.md)
- [Testing Guide](../../docs/extensions/testing-guide.md)
