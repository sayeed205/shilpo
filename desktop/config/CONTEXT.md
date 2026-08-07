# Configuration

Shell configuration: TOML loading and validation, XDG directory resolution, and
session state storage. Reads and validates extension references but owns no
extension runtime — it depends on `shilpo-ext-types` for ID validation, not on
the `shilpo-ext` runtime.

## Language

**Extension contribution reference** (`ExtensionContributionRef`):
The config-serialization form of a Canonical ID: `ext:<extension>/<contribution>`
as written in TOML config values. The underlying identifier is the Canonical ID
from `shilpo-ext-types`; the `ext:` prefix is the config language's namespacing
for extension references.
_Avoid_: canonical ID (the bare composite has no `ext:` prefix)

**Extension settings key**:
The extension ID used as a key under `extensions.settings` in config, validated
against the Extension ID rules so typos are caught at config-parse time.
_Avoid_: extension key, settings name
