# Shilpo TypeScript Extension SDK (`@shilpo/ext-sdk`)

Official TypeScript SDK for developing sandboxed WebAssembly extensions for the Shilpo Linux desktop
environment.

## Installation

`@shilpo/ext-sdk` is distributed via [JSR](https://jsr.io).

### Deno

```bash
deno add jsr:@shilpo/ext-sdk
```

### Bun

```bash
bunx jsr add @shilpo/ext-sdk
```

### npm

```bash
npx jsr add @shilpo/ext-sdk
```

### pnpm

```bash
pnpm dlx jsr add @shilpo/ext-sdk
```

### Yarn

```bash
yarn dlx jsr add @shilpo/ext-sdk
```

---

## Getting Started

### 1. Define Manifest (`extension.toml`)

```toml
schema_version = 1
id = "org.shilpo.sample"
name = "Sample Extension"
version = "0.1.0"
min_shilpo_version = "0.1.0"

[library]
path = "extension.wasm"

[contributions]
bar_widgets = [
  { id = "status", name = "Status Widget" }
]

[[capabilities]]
kind = "notifications:show"

[[capabilities]]
kind = "events:subscribe"
events = ["theme_changed", "workspace_changed"]

[[subscriptions]]
event = "theme_changed"

[[subscriptions]]
event = "workspace_changed"
```

### 2. Implement Extension (`src/extension.ts`)

```typescript
import {
  button,
  Colors,
  column,
  defineExtension,
  progress,
  row,
  style,
  text,
  toggle,
} from "@shilpo/ext-sdk";

let clickCount = 0;

const ext = defineExtension({
  onActivate(act, host) {
    host.notifications.show({
      title: "Sample Extension",
      body: "Activated successfully",
    });
    host.state.setString("status", "ready");
  },

  onDeactivate(_reason, host) {
    host.state.setString("status", "stopped");
  },

  onInput(event, host) {
    if (event.eventId === "btn_click") {
      clickCount += 1;
      host.state.setNumber("click_count", clickCount);
    }
  },

  view(contributionId, host) {
    if (contributionId !== "status") {
      return undefined;
    }

    return column({
      gap: 8,
      style: style({ padding: 12, background: Colors.surfaceContainer }),
      children: [
        row({
          children: [
            text("Sample Extension", { bold: true, style: style({ color: Colors.primary }) }),
          ],
        }),
        text(`Clicks: ${clickCount}`),
        toggle(true, "toggle_active"),
        progress(0.5),
        button("Click Me", "btn_click"),
      ],
    });
  },
});

export const activate = ext.activate;
export const deactivate = ext.deactivate;
export const onEvent = ext.onEvent;
export const view = ext.view;
```

### 3. Build WebAssembly Component

Compile the TypeScript extension into a WebAssembly Component model binary targeting the
`shilpo:extension@0.1.0` WIT world using the pinned QuickJS componentization backend:

```bash
npx --yes @bytecodealliance/jco componentize src/extension.ts \
  --wit node_modules/@shilpo/ext-sdk/wit \
  --world-name extension \
  --backend qjs \
  --backend-qjs-disable-async \
  -o extension.wasm
```

> **Note on Componentization Backend**: Shilpo extension tooling pins the QuickJS backend
> (`--backend qjs --backend-qjs-disable-async`). QuickJS produces lightweight (~1.7 MiB) synchronous
> WebAssembly components that load, validate, and execute in sub-second timeframes under Wasmtime,
> whereas default StarlingMonkey binaries are significantly larger and incur substantial JIT
> compilation overhead.

---

## Features

- **Declarative ViewTree Builders**: Fluent, type-safe constructors (`container`, `row`, `column`,
  `stack`, `grid`, `text`, `icon`, `image`, `button`, `toggle`, `slider`, `textInput`, `list`,
  `progress`, `badge`, `loadingIndicator`).
- **Typed `DataValue` Helpers**: Explicit tagged union handling without untyped JSON string bridges.
- **Host Import Facade**: Strongly typed APIs for state, secrets, notifications, clipboard, HTTP,
  filesystem, location, theme, and wallpaper.
- **Hermetic Testing**: Built-in `FakeHost` and `createTestHost` for isolated unit testing.
- **Cross-Runtime Portability**: Works out of the box under Deno, Bun, and Node.js ESM.

## Maintainer release

The TypeScript SDK is published to JSR by GitHub Actions. The one-time setup is:

1. Create or claim `@shilpo/ext-sdk` on [JSR](https://jsr.io) and link it to the `sayeed205/shilpo`
   GitHub repository.
2. Ensure the JSR package is configured to allow this repository's GitHub Actions workflow to
   publish.
3. Update the version in `deno.json` and commit the generated sources.
4. Push a tag named `typescript-sdk-v<version>`, matching the `deno.json` version exactly (for
   example, `typescript-sdk-v0.1.0`).

The workflow runs generation checks, formatting, lint, type checks, tests, and a package dry run
before publishing with GitHub Actions OIDC. No `JSR_TOKEN` secret is required. Publication is scoped
to `sdk/typescript` and JSR supplies provenance for the published package.
