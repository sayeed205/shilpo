# Shilpo Benchmarking Architecture

Shilpo uses a dual-engine benchmarking infrastructure:
- **[Criterion.rs](https://bheisler.github.io/criterion.rs/book/)**: Local and native wall-clock statistical profiling with HTML/JSON report generation.
- **[CodSpeed](https://codspeed.io/)**: Continuous CPU simulation and memory tracking with differential flamegraphs in CI.

---

## 1. Suite Ownership and Measured Boundaries

All benchmark targets live within their owning crates and measure public, production-relevant domain seams without timing fixture construction, process topology, or external services.

| Crate | Target | Benchmark Groups | Measured Boundary | Excluded from Timing |
| :--- | :--- | :--- | :--- | :--- |
| **`shilpo-theme`** (`core/theme`) | `theme` | `theme/resolve_variant`<br>`theme/generate_palettes` | • HCT chroma analysis & variant resolution<br>• M3 palette generation & token materialization (light/dark pair) | • Fixture seed preparation<br>• Terminal/display formatting |
| **`shilpo-ext-api`** (`core/ext-api`) | `identity`<br>`view_tree` | `identity/extension_id/valid`<br>`identity/extension_id/invalid`<br>`identity/contribution_id/valid`<br>`identity/contribution_id/invalid`<br>`identity/canonical_id/parse_valid`<br>`identity/canonical_id/parse_invalid`<br>`identity/canonical_id/new`<br>`view_tree/validate_valid`<br>`view_tree/validate_rejection` | • Identifier parsing, domain validation, and canonical composition<br>• `ViewTree::validate` traversal against canonical `ViewLimits` on 1, 64, 256, 1024, and 1025 node trees | • String/tree construction outside timing<br>• Result assertions outside iterations |
| **`shilpo`** (`desktop/shilpo`) | `config` | `config/deserialize`<br>`config/validate`<br>`config/parse_and_validate`<br>`config/resolve_layered` | • TOML deserialization into `ShellConfig`<br>• Full semantic validation of parsed configuration<br>• Combined parse + validation<br>• `ConfigResolver` initial layered resolution (`config.toml`, `conf.d/*.toml`, `overrides.toml`) | • Temporary directory setup and file writing<br>• OS cache flushing or privileged operations |
| **`shilpo-ext-runtime`** (`desktop/ext-runtime`) | `wasm` | `wasm/cold_load` | • Component compilation, WIT validation, linker/store setup, limits/fuel initialization, and guest instantiation | • `WasmRuntime` & engine creation<br>• Epoch ticker creation/drop<br>• Secret Service / LMDB / filesystem access<br>• Component unloading (executed outside elapsed time) |

---

## 2. Stable Benchmark Identifiers

CodSpeed and Criterion track historical trends using fixed, hierarchical identifier paths:

```text
theme/
├── resolve_variant/auto/<low_chroma|medium_chroma|high_chroma>
└── generate_palettes/m3/<seed>/<variant>

identity/
├── extension_id/valid/parse/<short|medium|near_limit>
├── extension_id/invalid/parse/<missing_segment|uppercase|invalid_char|leading_dash>
├── contribution_id/valid/parse/<short|medium|near_limit>
├── contribution_id/invalid/parse/<leading_dash|uppercase|invalid_char|empty>
├── canonical_id/parse_valid/parse/<short|medium|near_limit>
├── canonical_id/parse_invalid/parse/<no_slash|multiple_slashes|invalid_ext|invalid_contrib>
└── canonical_id/new/composed

view_tree/
├── validate_valid/nodes/<1|64|256|1024>
└── validate_rejection/nodes/1025

config/
├── deserialize/valid_full
├── validate/valid_full
├── parse_and_validate/valid_full
└── resolve_layered/initial_resolution

wasm/
└── cold_load/sdk_fixture
```

---

## 3. Running Benchmarks Locally

### Using the `scripts/bench.sh` Runner

The `scripts/bench.sh` utility provides a unified interface for developers and CI pipelines:

```bash
# Fast smoke check: compiles and executes every benchmark target once in test mode
./scripts/bench.sh smoke

# Run core domain benchmarks (theme, identity, view_tree)
./scripts/bench.sh core

# Run configuration benchmarks
./scripts/bench.sh config

# Run Wasm extension cold load benchmarks
./scripts/bench.sh wasm

# Run all benchmark suites
./scripts/bench.sh all

# Forward custom Criterion flags (e.g. quick mode)
./scripts/bench.sh core -- --quick
```

### Direct Cargo Commands

You can run individual benchmarks directly with standard `cargo`:

```bash
# Theme benchmarks
cargo bench -p shilpo-theme --bench theme

# Extension API identity and ViewTree benchmarks
cargo bench -p shilpo-ext-api --bench identity
cargo bench -p shilpo-ext-api --bench view_tree

# Configuration benchmarks
cargo bench -p shilpo --bench config

# Wasm cold load benchmarks
cargo bench -p shilpo-ext-runtime --bench wasm

# Run a specific benchmark target in fast test/smoke mode
cargo bench -p shilpo-theme --bench theme -- --test
```

---

## 4. Continuous Benchmarking and CI Architecture

### CodSpeed Continuous Benchmarking (`.github/workflows/codspeed.yml`)

- **Triggers**: Pull requests targeting `main`, pushes to `main`, and `workflow_dispatch`.
- **Mechanism**: Instruments the core benchmark binaries using CodSpeed simulation mode.
- **Benefits**:
  - Low-variance CPU measurements that are comparable across ordinary CI runners.
  - Differential flamegraphs and public performance history for pull requests.
- **Permissions**: Minimal least-privilege tokens (`contents: read`, `id-token: write` for OIDC authentication).

### Native Wall-Clock Benchmarks (`.github/workflows/benchmarks.yml`)

- **Triggers**: Weekly schedule on `main` (Sundays at 00:00 UTC) and manual `workflow_dispatch`.
- **Environment Metadata**: Each run captures commit SHA, Rust version, OS, kernel, CPU model, timestamp, and fixture identity in `target/criterion/metadata/environment.json`.
- **Artifact Retention**:
  - Manual dispatches: **30 days**.
  - Scheduled `main` branch runs: **90 days**.
- **Reports**: Machine-readable JSON data and interactive HTML reports uploaded under `target/criterion/`.

---

## 5. Failure and Gating Policy

- **Report-Only Performance Policy**: Benchmark timing changes and performance deltas are informational and do not fail pull requests. Performance gates may be introduced once sufficient variance baseline history is established.
- **Strict Infrastructure Gating**: Pull requests fail immediately if:
  - Benchmark targets fail to compile in release benchmark profile.
  - Benchmark fixtures are missing, corrupt, or invalid.
  - Execution times out or panics.
  - Generated reports are malformed or missing.
  - Tracked source files are dirtied or uncommitted files are produced.

Pull-request smoke runs the core, config, and Wasm suites in parallel Criterion test-mode jobs. Core has an 8-minute command budget; config and Wasm each have a 20-minute command budget, all inside 30-minute jobs. CodSpeed installation, compilation, and execution each have their own smaller command timeout inside a 30-minute job. Scheduled/manual native measurement has a 50-minute command budget inside a 60-minute job. A timeout is reported as an infrastructure failure, never as a performance regression.
