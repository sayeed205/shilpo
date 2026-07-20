# Agent Guidelines for Shilpo (`shilpo-ui`)

Welcome! This document serves as a guide for AI agents and coding assistants working on the `Shilpo` codebase.

---

## 1. Project Architecture Overview

`Shilpo` is a modern, high-performance desktop UI component library built
for [GPUI](https://github.com/zed-industries/zed), inspired by **Material Design 3 (M3 Expressive / Material You)**
design systems.

### Workspace Structure

- **[`shilpo-ui`](crates/ui)**: Core desktop UI component library (`crates/ui`).
- **[`shilpo-macros`](crates/macros)**: Procedural macros (`icon_named!`, plot traits, etc.) (`crates/macros`).
- **[`shilpo-assets`](crates/assets)**: Internal demo SVG icons and asset loader primitives (`crates/assets`). **Note**:
  This crate is unpublished; applications bring their own asset loader.
- **[`storybook`](apps/storybook)**: Interactive desktop gallery app for exploring and testing UI components (
  `apps/storybook`).

---

## 2. Using the `rtk` Prefix for Command Execution

> **CRITICAL RULE FOR AI AGENTS**:
> Whenever executing `cargo` commands in the terminal (clippy, testing, building, coverage), **always prefix the command
with `rtk`**.

### Recommended `rtk` Commands

| Purpose              | Standard Command                         | **Agent Command (Use This)**                 |
|:---------------------|:-----------------------------------------|:---------------------------------------------|
| **Linting / Clippy** | `cargo clippy --workspace --all-targets` | `rtk cargo clippy --workspace --all-targets` |
| **Fast Testing**     | `cargo nextest run --workspace`          | `rtk cargo nextest run --workspace`          |
| **Standard Testing** | `cargo test -p shilpo-ui --lib`          | `rtk cargo test -p shilpo-ui --lib`          |
| **Code Coverage**    | `cargo llvm-cov --workspace`             | `rtk cargo llvm-cov --workspace`             |
| **Workspace Build**  | `cargo build --workspace`                | `rtk cargo build --workspace`                |
| **Storybook App**    | `cargo run -p storybook`                 | `rtk cargo run -p storybook`                 |

---

## 3. Tooling Standards: Clippy, Nextest, and LLVM-Cov

### Clippy (`rtk cargo clippy --workspace --all-targets`)

- **Zero Warnings Policy**: All code added or modified must pass `rtk cargo clippy --workspace --all-targets` with **0
  errors and 0 warnings**.
- Keep doc comments updated and clean up unused imports.

### Nextest (`rtk cargo nextest run`)

- `cargo-nextest` is the preferred test runner for running tests in parallel.
- Run unit tests for individual crates using:
  ```bash
  rtk cargo nextest run -p shilpo-ui
  ```

### LLVM Coverage (`rtk cargo llvm-cov`)

- Use `llvm-cov` to audit code coverage when implementing or refactoring core components:
  ```bash
  rtk cargo llvm-cov --workspace --summary-only
  ```

---

## 4. Coding & Design System Guidelines

1. **Material 3 Expressive Aesthetics**:
    - Use curated M3 theme color tokens (`cx.theme().primary`, `cx.theme().surface_container`,
      `cx.theme().on_surface_variant`, etc.) instead of hardcoded colors.
    - Support M3 motion curves (e.g. M3 Emphasized Easing `cubic_bezier(0.2, 0.0, 0.0, 1.0)`)
      over $200\text{ms}$–$300\text{ms}$.
2. **GPUI Element Patterns**:
    - Implement GPUI traits (`IntoElement`, `RenderOnce`, `Sizable`, `Selectable`, `Disabled`) consistently.
    - Support mouse interaction safety (e.g. `cx.stop_propagation()` on mouse down for draggable titlebars).
3. **Interactive Documentation**:
    - When introducing or modifying UI components, add interactive stories in `apps/storybook/src/stories/`.

---

## 5. Documentation Maintenance

- Standard `README.md` files aimed at human developers must use standard `cargo` commands (e.g.,
  `cargo run -p storybook`).
- Do not add `rtk` prefixes to public `README.md` files; keep `rtk` instructions internal to `AGENTS.md` and agent
  workflows.
