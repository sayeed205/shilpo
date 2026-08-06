# Cross-platform apps in separate repositories

Future cross-platform Shilpo apps (file manager, etc.) that are not part of the desktop shell environment live in their
own repositories, not in this workspace. They consume `shilpo-ui` and `shilpo-theme` as published crate dependencies
from crates.io, not as workspace path dependencies.

This keeps the shell workspace focused (already 12+ crates) and gives each app its own release cycle, CI, and issue
tracker. Shell-integrated apps (`shilpo-settings`, `shilpo-cli`) stay in this workspace because they share the desktop
ecosystem's internal crates (`services`, `config`, `ext`). The dividing line: if an app depends on `desktop/` crates, it
belongs here; if it only depends on `core/` crates, it gets its own repo.
