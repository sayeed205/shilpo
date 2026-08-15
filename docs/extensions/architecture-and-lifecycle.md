# Extension Architecture & Lifecycle Guide

This guide details the internal runtime architecture, lifecycle state transitions, declarative UI model, and data coordination of Shilpo extensions.

---

## 1. Extension Lifecycle

Shilpo extensions execute as WebAssembly components sandboxed inside Wasmtime workers. Extensions implement four core export functions defined in `shilpo:extension@0.1.0`:

```
┌──────────────────────────────────────────────────────────────┐
│                    Extension Lifecycle Flow                  │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│                  ┌──────────────────────┐                    │
│                  │      Instantiate     │                    │
│                  └──────────┬───────────┘                    │
│                             │                                │
│                             ▼                                │
│                  ┌──────────────────────┐                    │
│                  │   activate(origin)   │                    │
│                  └──────────┬───────────┘                    │
│                             │                                │
│                             ▼                                │
│               ┌────────────────────────────┐                 │
│         ┌────►│ on_event(event) / view(id) │◄────┐           │
│         │     └─────────────┬──────────────┘     │           │
│         │ User input /      │ System event /     │ State     │
│         │ UI click          │ Timer tick         │ mutation  │
│         └───────────────────┴────────────────────┘           │
│                             │                                │
│                             ▼                                │
│                 ┌────────────────────────┐                   │
│                 │   deactivate(reason)   │                   │
│                 └────────────────────────┘                   │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### `activate(activation: types.Activation)`
Called when the extension is first loaded or mounted by the shell. `activation.origin` identifies whether activation was triggered by `shell-startup`, `user-enable`, or `hot-reload`. Extensions initialize in-memory structures and register state watches here.

### `deactivate(reason: types.DeactivateReason)`
Called when the extension is disabled or during orderly shell shutdown. Reasons include `user-requested`, `shell-shutdown`, `reload`, or `error-quarantine`. Extensions release resources and save persistent state here.

### `on_event(event: events.ExtensionEvent)`
Dispatches inbound events into the extension:
- `input`: UI button clicks, toggle switches, text inputs, slider changes.
- `state-value`: Updates to watched state keys.
- `palette-generated`: System Material 3 theme palette recalculations.
- `wallpaper-changed`: Desktop wallpaper source modifications.

### `view(contribution_id: string) -> Option<view.ViewTree>`
Renders the declarative UI tree for any declared visual contribution (`bar_widgets`, `bar_menus`, `desktop_widgets`, `settings_pages`, `side_panels`).

---

## 2. Declarative ViewTree UI Model

Shilpo uses a flat, indexed node array `ViewTree` representing declarative M3 Expressive UI layouts.

```typescript
import { buildViewTree, button, column, icon, row, text } from "@shilpo/ext-sdk";

export function renderMyWidget(count: number) {
  return buildViewTree(
    column({
      gap: 8,
      style: { padding: 12 },
      children: [
        row({
          alignItems: "center",
          gap: 6,
          children: [
            icon("dashboard", { size: 18 }),
            text("Dashboard Widget", { bold: true }),
          ],
        }),
        text(`Counter: ${count}`),
        button("Increment", "btn-increment", { style: { padding: 6 } }),
      ],
    }),
  );
}
```

### Core Primitives
- **Containers**: `row`, `column`, `stack`, `grid` with flexible `gap`, `alignItems`, `justifyContent`, and `style`.
- **Text & Media**: `text`, `icon`, `image`, `badge`, `divider`, `spacer`.
- **Interactive Controls**: `button`, `iconButton`, `toggle`, `slider`, `textInput`.
- **Feedback**: `progress`, `loadingIndicator`.

---

## 3. Extension Key-Value State Store

Extensions store persistent data in a scoped, durable key-value store namespaced by extension ID:

```typescript
// Read state
const snapshot = host.state.read("user_preferences");

// Write state atomically with monotonic revision
host.state.write("user_preferences", DataValue.text(JSON.stringify(prefs)));

// Watch a key for real-time reactivity
const registration = host.state.watch("user_preferences");
```

- Keys: Up to 256 bytes UTF-8.
- Storage: Up to 256 keys and 4 MiB total payload per extension.
- Safety: Secret handles (`SecretRef`) are strictly rejected by the state store to prevent accidental leakage.

---

## 4. Degraded Host Fallbacks

In robust extensions, all host API calls (`state`, `clipboard`, `notifications`, `theme`) should be wrapped with graceful fallbacks. If an optional host capability is unavailable or errors, the extension continues operating using in-memory state without crashing or trapping.
