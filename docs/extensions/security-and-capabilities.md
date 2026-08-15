# Extension Security & Capability Model

Shilpo enforces a defense-in-depth security model combining WebAssembly sandboxing, default-deny WASI execution, explicit manifest capability declaration, and user-prompted authorization.

---

## 1. WebAssembly Sandboxing

All Shilpo extensions run inside isolated `wasmtime` component instances with the following guarantees:

- **Isolated Memory**: No access to host memory, other extensions, or the desktop process address space.
- **Default-Deny WASI**: WASI preopens are empty (no direct host filesystem access), raw sockets are prohibited, and subprocess execution is blocked.
- **Resource Limits**: Epoch-based execution timeouts, fuel consumption bounds, and max component size limits prevent unbounded loops or hangs.

---

## 2. Least-Privilege Capability Declaration

Extensions must declare the exact, minimal set of capabilities required for their features. Wildcard scopes or unused capabilities are rejected during review and validation.

### Capability Scopes Reference

| Capability | Manifest Declaration | Description |
| :--- | :--- | :--- |
| **Events Subscribe** | `kind = "events:subscribe"`, `events = [...]` | Authorizes subscription to specific system event categories. |
| **Theme Read** | `kind = "theme:read"` | Authorizes reading current Material 3 color palettes and light/dark modes. |
| **Theme Set Source** | `kind = "theme:set_source"` | Authorizes changing the active system theme seed color. |
| **Notifications Show** | `kind = "notifications:show"` | Authorizes displaying desktop user notifications. |
| **Clipboard Read** | `kind = "clipboard:read"` | Authorizes reading text from the system clipboard. |
| **Clipboard Write** | `kind = "clipboard:write"` | Authorizes writing text to the system clipboard. |
| **Wallpaper Read** | `kind = "wallpaper:read"` | Authorizes reading active wallpaper configuration. |
| **Wallpaper Set** | `kind = "wallpaper:set"`, `sources = [...]` | Authorizes changing the desktop wallpaper. |
| **Actions Invoke** | `kind = "actions:invoke"`, `actions = [...]` | Authorizes invoking registered commands or other extension actions. |
| **Network HTTP** | `kind = "network:http"`, `hosts = [...]` | Authorizes making HTTP requests to explicit host whitelists. |
| **Filesystem Read** | `kind = "filesystem:read"`, `paths = [...]` | Authorizes reading files within declared directory paths. |
| **Filesystem Write** | `kind = "filesystem:write"`, `paths = [...]` | Authorizes writing files within declared directory paths. |
| **Location Read** | `kind = "location:read"` | Authorizes requesting coarse geographic coordinates. |
| **Secrets** | `kind = "secrets"`, `purposes = [...]` | Authorizes storing and retrieving encrypted credentials. |

---

## 3. Non-Disruptive Execution Policy

- **No Startup Disruption**: Notifications, clipboard modifications, and wallpaper changes must only occur in direct response to user interaction (e.g. clicking a menu action), never on startup or activation.
- **Transparent Prompting**: When an extension requests privileged capabilities (e.g. network hosts, filesystem paths), the Shilpo shell presents a clear authorization modal to the user before granting access.
