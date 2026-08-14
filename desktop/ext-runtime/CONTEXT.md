# Shilpo Extension Runtime Context (`shilpo-ext-runtime`)

## Domain Vocabulary

- **Extension Runtime**: Worker-owned execution environment for sandboxed Wasmtime/WASI guest components and explicitly
  trusted local script bundles. The two runtime kinds share contribution snapshots but not authority or guest APIs.
- **Authorized Host Effect (`AuthorizedHostEffect`)**: Runtime-checked and policy-validated effect token issued when a requested `HostEffect` matches declared capabilities and stored user grants.
- **Extension Catalog (`ExtensionCatalog`)**: Storage manager for installed extensions, signature verification, capability grant persistence, and registry index synchronization.
- **Worker Process Protocol**: Framed child process protocol (`shilpo extension-host`) used to isolate Wasmtime execution from the GPUI desktop shell process.
- **Trusted Local Script (`TrustedLocalScript`)**: Local-only, unsandboxed executable bundle discovered under the
  Shilpo script source. It may emit read-only bar-widget views but has no WIT imports, host effects, catalog operations,
  search providers, or path into the WASM capability model.
- **Search Provider Contribution (`SearchProviderContribution`)**: WASM extension contribution declaring a search provider that feeds results to the Shell's Overview and search UI. Trusted local scripts cannot provide search providers.
- **Script Runtime (`ScriptRuntime`)**: Deep worker-owned module that validates script bundles, schedules poll/stream
  tasks, owns process groups, decodes bounded records, retains last-valid views, and projects script status.
