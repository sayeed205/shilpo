# Shilpo Extension Runtime Context (`shilpo-ext-runtime`)

## Domain Vocabulary

- **Extension Runtime**: Wasmtime/WASI execution engine for compiling, instantiating, and isolating guest components.
- **Authorized Host Effect (`AuthorizedHostEffect`)**: Runtime-checked and policy-validated effect token issued when a requested `HostEffect` matches declared capabilities and stored user grants.
- **Extension Catalog (`ExtensionCatalog`)**: Storage manager for installed extensions, signature verification, capability grant persistence, and registry index synchronization.
- **Worker Process Protocol**: Framed child process protocol (`shilpo extension-host`) used to isolate Wasmtime execution from the GPUI desktop shell process.
