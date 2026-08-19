# ADR-0006: Revisioned Service Domain Ports and Supervision Contract

- **Status:** Accepted
- **Date:** 2026-08-11
- **Deciders:** Shilpo Core Architecture Team
- **Related Issues:** #85, #126, #127, #135, #136
- **Related ADRs:** ADR-0005

---

## 1. Context

Shilpo desktop environment services (device daemon, Wayland compositor integration, notifications, clipboard, network,
brightness, power profiles, media, etc.) require consistent operational semantics. Historically, service construction
was fragmented across the codebase:

- The device domain used `DomainLifecycle` and revision numbers without an explicit owner generation fence.
- The compositor integration used a custom `CompositorConnection` state machine and bounded command broker.
- Device command channels and several service worker channels used unbounded channels.
- Connection loss and failure recovery led to varied fallback behaviors, occasionally dropping safe cached payloads or
  attempting uncoordinated retries.

Standardizing system domain operations requires defining clear rules for ownership, lifecycle, freshness, command
execution, backpressure, supervision, and offline projections. However, creating a universal generic production trait
(such as `DomainPort<T>` or `Supervisor<T>`) or a monolithic service-locator bag (`ShellServices`) would erase
domain-specific payloads, introduce unnecessary dynamic dispatch (`dyn Any`), force string-based command interfaces or
unstructured `serde_json::Value` payloads, and couple independent Linux desktop subsystems.

This decision establishes one shared operational contract—the **Revisioned Service Domain Port**—that standardizes
operational behavior while preserving narrow, strongly typed domain interfaces.

---

## 2. Decision

We accept the revisioned service domain port and supervision contract as the standard operational model for long-lived
system service domains in Shilpo.

Key principles of this decision:

1. **Domain port vocabulary:** Long-lived system services expose narrow, domain-specific Rust types adhering to standard
   operational semantics (authoritative owner, atomic snapshot, watch stream, typed commands, terminal outcomes,
   degraded projections).
2. **Lifecycle and supervisor separation:** Consumer-facing lifecycle states (`Unavailable`, `Connecting`, `Ready`,
   `Reconnecting`, `Degraded`) are explicitly distinct from operational supervisor states (`Starting`, `Running`,
   `Backoff`, `Quarantined`, `Stopping`, `Stopped`).
3. **Freshness via DomainVersion:** Snapshots and updates are versioned using
   `DomainVersion { owner_generation: u64, revision: u64 }` with strict lexicographical ordering, fencing out stale
   owners after process/session restarts and permitting revision reset on owner replacement.
4. **Snapshot and degraded projection rules:** Safe cached payloads are retained during reconnection or degraded
   operation. Unavailable states do not fabricate readiness.
5. **Commands, outcomes, and convergence:** Accepted commands carry unique `CommandId`s and yield exactly one terminal
   outcome (`Applied`, `ReconciledApplied`, `Rejected`, `TimedOut`, `Cancelled`). Backend acknowledgement alone is
   insufficient for convergence-based commands.
6. **Bounded mailboxes:** Unbounded command queues are forbidden. Every domain command mailbox has a positive capacity
   and enforces either `Lossless` (rejecting overflow with `Overloaded`) or `ReplaceLatest` (superseding pending
   commands with matching keys).
7. **Baseline supervisor policy:** Supervisors enforce exponential backoff starting at `250 ms` up to `30 s`, tripping
   into `Quarantined` after five unexpected failures inside a rolling `60` s window, and clearing the failure window
   after `5 minutes` of continuous stability.
8. **Process vs. In-process variants:** Both variants preserve identical consumer semantics while using appropriate IPC
   or handle transport.

---

## 3. Domain port vocabulary

A service domain port is a semantic operational contract, not a single generic production Rust trait. Each long-lived
domain (audio, brightness, network, compositor, notifications, etc.) exposes concrete domain-specific types.

Every domain port exposes:

- **Authoritative Owner:** Exactly one process or task managing backend I/O for the domain.
- **Atomic Snapshot:** A single, consistent data structure containing current lifecycle, `DomainVersion`, domain
  payload, and optional `last_error` diagnostics.
- **Latest-Value Watch Stream:** A subscription mechanism for consumers to observe atomic snapshot updates.
- **Typed Commands:** Strongly typed request structures identified by a unique `CommandId`.
- **Terminal Outcomes:** Exactly one terminal outcome per accepted command.
- **Deterministic Offline/Degraded Projection:** Fallback states for when the owner is absent or impaired.

We explicitly reject universal production traits (`DomainPort<T>`), string-based command dispatchers,
`serde_json::Value` payload bags, `dyn Any` registries, and `ShellServices` service locators. Consumers receive only the
narrow, typed domain ports they require.

---

## 4. Ownership and narrow consumer dependencies

Consumers (Shell widgets, Settings app panels, CLI tools) depend directly on narrow domain-specific ports (e.g.,
`AudioPort`, `CompositorPort`, `BrightnessPort`).

### Current implementations & migration mapping

- **Device (`shilpo-device`):** Existing partial exemplar using `DomainLifecycle` and `revision`. Will be updated under
  #85 to incorporate `owner_generation` fencing and bounded mailboxes.
- **Compositor (`shilpo-services::compositor`):** Existing partial exemplar with bounded command broker and convergence.
  Will be updated under #85 to align with `DomainVersion` lexicographical rules.
- **Notifications & Clipboard (`shilpo-services`):** Production migration targets under #85 to replace ad-hoc shell
  retries and unbounded channels with supervised domain ports. Clipboard history work in #126 extends the migrated
  clipboard domain and must retain this contract.
- **Idle, Lock, & Polkit:** Future service domains created by #127, #135, and #136 respectively. Lock screen work in
  #135 depends on the idle domain from #127.

---

## 5. Domain lifecycle state machine

The consumer-facing lifecycle represents the availability and authoritative status of domain data:

```text
Unavailable ──► Connecting ──► Ready
    ▲                │           │
    │                ▼           ▼
    └─────────── Reconnecting ◄──┤
                     │           │
                     ▼           ▼
                  Degraded ◄─────┘
```

### Consumer Lifecycle States

- `Unavailable`: No live owner exists, or data cannot be claimed as authoritative.
- `Connecting`: Initial startup of the domain owner is in progress.
- `Ready`: Owner is live and snapshot payload is authoritative.
- `Reconnecting`: Prior owner lost connection; recovery is in progress while safe cached payload is retained.
- `Degraded`: Owner is partially functional but cannot provide full capability; safe payload is retained and
  `last_error` is set.

---

## 6. Supervisor state machine and policy

Operational supervision manages owner task/process startup, restarts, backoff, and quarantine independently of consumer
UI states:

```text
Starting ──► Running ──► Backoff { attempt, retry_at } ──► Starting
  │           │              │ (after 5 failures in 60 s)
  │           │              ▼
  │           └─────────► Quarantined
  │                          │
  └─── (any state) ──► Stopping ──► Stopped
```

### Supervisor States and Mapping to Consumer Lifecycle

- `Starting`: Maps to `Connecting` on first launch, or `Reconnecting` after prior readiness.
- `Running`: Maps to `Ready` (or `Degraded` if the owner reports partial backend failure).
- `Backoff`: Maps to `Reconnecting` if a safe cached payload exists; otherwise `Unavailable`.
- `Quarantined`: Maps to `Unavailable` (retaining safe cached payload or fallback, but never claiming `Ready`).
- `Stopping` / `Stopped`: Maps to `Unavailable`.

### Baseline Supervisor Policy Constants

- **Initial backoff:** `250 ms`
- **Multiplier:** `2` (exponential sequence: 250 ms, 500 ms, 1000 ms, 2000 ms, 4000 ms...)
- **Maximum backoff:** `30 s`
- **Trip limit:** five unexpected failures inside a rolling `60` s window triggers `Quarantined`.
- **Stable reset:** `5 minutes` of continuous `Running` or `Ready` state clears the rolling failure window (while
  preserving the lifetime session restart counter for telemetry).
- **Quarantine policy:** Remains `Quarantined` until an explicit user/system reset or containing-process restart.

### Relationship to ADR-0005

ADR-0005 defines specialized executable process supervision constants for the Wasmtime extension-host child process (250
ms, 1 s, 4 s; trip after three failures in 60 seconds). ADR-0005 remains authoritative for extension-host process
supervision. ADR-0006 governs service domain ports.

---

## 7. DomainVersion and stale-update rules

Every domain snapshot and update carries a `DomainVersion`:

```rust
pub struct DomainVersion {
    pub owner_generation: u64,
    pub revision: u64,
}
```

### Lexicographical Comparison Rule

`DomainVersion` uses strict lexicographical ordering:

1. Compare `owner_generation` first.
2. If `owner_generation` is equal, compare `revision`.

`DomainVersion(g2, r_0) > DomainVersion(g1, r_100)` whenever `g2 > g1`.

### Generation Rules

- `owner_generation` increments once per owner start or reconnect attempt. It is session-local and unpersisted.
- In-process supervisors assign the generation before spawning owner tasks.
- Process-owned clients assign the generation when establishing a new DBus unique-name owner connection.
- `revision` starts at zero for a new owner generation and increases whenever the authoritative domain projection
  changes.
- **Revision Reset:** A new owner generation permits revision to reset to zero.

### Stale and Conflict Handling

- Accept an update **only** if its `DomainVersion` is strictly greater than the current snapshot version.
- If an update has identical `DomainVersion`:
    - If payload and lifecycle match identically, ignore idempotently.
    - If payload or lifecycle differs, reject as a contract conflict and increment `stale_updates` telemetry.
- If an update has an older generation or older revision (`DomainVersion < current`), reject as stale and increment
  `stale_updates` telemetry.
- If an update claims a future generation that the supervisor/client has not installed, reject it as an uninstalled
  generation; updates never establish their own owner generation.

---

## 8. Snapshot and degraded projection rules

A domain snapshot conceptually follows:

```rust
pub struct DomainSnapshot<Payload> {
    pub version: DomainVersion,
    pub lifecycle: DomainLifecycle,
    pub payload: Payload,
    pub last_error: Option<DomainError>,
}
```

### Degraded Rules

- Initial state starts at `Unavailable`, version `(0, 0)`, default domain fallback, and no fabricated availability.
- Connection loss transitions lifecycle to `Reconnecting` or `Unavailable` while retaining the last known safe payload.
- Security and permission state (e.g., Polkit authorizations, screen lock state) MUST fail closed during reconnecting or
  degraded states.
- Reconnection installs a new `owner_generation` before updates are accepted.
- Subscribers observe atomic snapshots via latest-value watch streams. Slow subscribers skip intermediate updates and
  converge directly to the latest accepted version.

---

## 9. Commands, mailbox policy, convergence, and terminal outcomes

Commands are identified by a unique `CommandId` and carry domain-specific parameters.

### Terminal Outcomes

Every accepted command terminates exactly once with one of these outcomes:

- `Applied { version }`: State change confirmed by authoritative snapshot update.
- `ReconciledApplied { version }`: Acknowledgement was delayed or lost, but state change was proven by subsequent
  snapshot observation.
- `Rejected { reason }`: Rejected prior to execution due to validation, permissions, unavailablity, or queue overload
  (`Overloaded`).
- `TimedOut { last_observed_version }`: State change convergence was not observed before the deadline.
- `Cancelled { reason }`: Command invalidated by shutdown, reconnect, owner replacement, or replacement by a newer
  command (`Superseded`).

Backend acknowledgement alone is NOT sufficient to complete a command when state convergence is observable.

### Bounded Mailbox Policies

Unbounded command channels are forbidden. Every command mailbox has an explicit positive capacity and uses one of two
policies:

1. `Lossless`: FIFO queue. When full, rejects new commands immediately with typed `Overloaded` rejections without
   dropping previously accepted commands.
2. `ReplaceLatest { key }`: Maintains at most one pending command per key. A newer command replaces an older pending
   command with the same key, cancelling the older command with `Cancelled::Superseded`. Commands with different keys do
   not replace each other.

Owner replacement cancels all pending and in-flight commands from previous generations.

---

## 10. Process-owned versus in-process domains

Consumer semantics remain identical regardless of owner deployment:

- **Process-owned (DBus):** Typed DBus client serves as the port. DBus well-known name / unique-owner changes set
  `owner_generation`. DBus signals carry `revision`. Client manages reconnect and degraded projections.
- **In-process (Task):** Narrow cloneable handle serves as the port. Supervisor task owns lifecycle and publishes watch
  snapshots and command outcomes. `owner_generation` is assigned on task spawn.

### 10.1. Owner replacement and cancellation semantics

Not every domain port has an externally-restartable owner independent of its broker/supervisor — process-owned (D-Bus)
ports do, while in-process single-owned ports (like compositor) do not. Reconnection in in-process domains occurs
entirely within the long-lived backend, producing `CancellationReason::Reconnect` rather than
`CancellationReason::OwnerReplaced`.

Domain ports declare which `CancellationReason` they produce on owner replacement via `owner_replacement_reason()` on
`DomainPortDriver`, rather than the conformance suite assuming uniformity (see #235).

---

## 11. Observability

Domain ports expose standardized telemetry metrics:

- `owner_generation`: Current owner generation count.
- `current_queue_depth`: Number of pending commands in mailbox.
- `queue_capacity`: Maximum capacity of command mailbox.
- `overloads`: Total number of command rejections due to mailbox capacity overflow.
- `supersessions`: Total number of pending commands superseded under `ReplaceLatest`.
- `restarts`: Lifetime count of owner restart attempts.
- `stale_updates`: Total number of rejected stale or conflicting updates.
- `last_error`: Diagnostics string for the most recent error.

---

## 12. Conformance testing

A test-only executable reference model and reusable test harness are provided under `desktop/services/tests/support/`
and `desktop/services/tests/domain_port_contract.rs`.

The test harness enforces 20 deterministic scenarios:

1. Initial projection is deterministic and unavailable.
2. Initial start follows `Unavailable -> Connecting -> Ready`.
3. Reconnect retains safe payload and records last error.
4. Strictly newer revision in same generation is accepted.
5. Stale generation is rejected.
6. Stale revision is rejected.
7. Conflicting payload at same version is rejected and diagnosed.
8. New owner generation permits revision reset.
9. Slow subscriber converges to latest atomic snapshot.
10. Accepted command receives exactly one terminal outcome.
11. Backend acknowledgement alone does not complete convergence command.
12. Lossless mailbox rejects overflow without dropping accepted commands.
13. Replace-latest supersedes pending command with same key and emits terminal cancellation.
14. Different replace-latest keys do not replace each other.
15. Owner replacement cancels old-generation pending/in-flight commands (per-port cancellation reason).
16. Backoff is exponential from 250 ms and capped at 30 seconds.
17. Five failures inside 60 seconds enter quarantine.
18. Five minutes stable clears rolling failure window but preserves session restart telemetry.
19. Quarantine requires explicit reset or containing-process restart.
20. Telemetry reports generation, queue depth/capacity, overloads, supersessions, restarts, stale updates, and last
    error.

The tests use a manual controllable clock (`ManualClock`) and require no real wall-clock sleeps, DBus sessions, Wayland
compositors, network access, or GPUI runtimes.

---

## 13. Consequences

### Positive

- Unified operational model for all desktop service domains.
- Predictable restart, exponential backoff, quarantine, and stale-update fencing.
- Mailbox bounds prevent memory growth under load.
- Strongly typed, narrow domain interfaces prevent monolithic coupling.

### Negative

- Require existing services (device, compositor) to adopt `owner_generation` fencing.

---

## 14. Rejected alternatives

- **Universal production generic trait (`DomainPort<T>`):** Erases concrete domain semantics and introduces dynamic
  dispatch overhead.
- **Monolithic `ShellServices` bag:** Reintroduces global service-locator anti-pattern.
- **Packing generation and revision into a single u64:** Obscures generation boundary and breaks revision reset on
  restart.
- **Wall-clock sleeps in tests:** Causes flaky and slow test suites; replaced by `ManualClock`.
- **Unbounded command queues:** Causes potential OOM or resource exhaustion under load.

---

## 15. Follow-up implementation boundaries

- **#85 (Service Migration):** Owns production migration of device daemon, Wayland compositor, notifications, clipboard,
  and Shell service polling to this contract.
- **#126:** Clipboard history must extend the clipboard domain without bypassing this contract.
- **#127:** Idle management must implement an idle domain using this contract.
- **#135:** Lock screen must implement a lock domain using this contract and depends on the idle domain from #127.
- **#136:** The Polkit agent must implement its service domain using this contract.
- **ADR-0005:** Extension-host process supervision policy constants remain specialized as defined in ADR-0005.
