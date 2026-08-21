# ADR-0018: Extension Registry Distribution and Publication Trust

- **Status**: Accepted

## Context

Shilpo has a complete extension *installation* client — signed registry indexes, per-release
signature verification, package hashes, install receipts, capability grants — but no
*distribution* infrastructure. Nothing publishes an index, and `shilpo ext install` has
nowhere to install from.

The client already fixes much of the shape: `RegistrySource` addresses a source by
`index_url` plus a root public key, `SignedRegistryIndex` wraps a `RegistryIndex` of
`RegistryRelease` records, and verification is Ed25519 over both the index envelope and
each release. What was undecided is everything on the publisher side: where submissions
land, who builds the artifact, who signs it, where the bytes live, and what "official"
means once first-party and third-party extensions share a distribution channel.

Three constraints shaped the options.

**The runtime addresses extensions globally.** `CanonicalId` is documented as the globally
unique address of a contribution and is exactly `extension_id/contribution_id`; its parser
rejects a second path segment. Bare `ExtensionId` is the key across the worker protocol,
circuit breaker, action dispatcher, contribution registration, configuration references,
secrets, state and shell UI. Any distribution model that would require two installed copies
of one extension id implies redesigning that address space.

**Trusted local scripts are not distributable.** [ADR-0016](0016-wit-extension-contract.md)
already establishes that script bundles are local-only, visibly unsandboxed, and excluded
from registry, update and grant paths. A registry that walks a repository of extensions
must not quietly acquire a path for them.

**Whoever builds the artifact is the only party who can sign it.** If CI compiles the WASM
from submitted source, the bytes do not exist until after the merge and the submitting
author never sees them. An author-held publisher key and CI-built artifacts are mutually
exclusive.

Prior art was surveyed. Noctalia distributes plugins as source over git with no signing at
all, which works because their plugins are text; Shilpo ships WASM binaries, so git as
transport would grow every clone without bound. Vicinae runs a real backend service on a
VPS, gaining download counts and trending at the cost of an uptime obligation, a hosting
bill, and a live attack surface on the path that delivers executable code. Vicinae's
stronger idea is that the store never accepts a publisher-supplied binary: CI builds it
from reviewed source.

## Decision

### 1. One repository, static index, no service

`shilpo-rs/extensions` is the extension registry. Every extension lives in `extensions/<id>/`
as source; that directory is the only path the index generator scans. Trusted local scripts
live outside it under `local-scripts/` and have no distribution path, per ADR-0016.

The generated index is committed to the repository and served over HTTPS. WASM artifacts are
GitHub Release assets. There is no server, no database, and no object storage. Consequently
there are no download counts and no trending — a static index cannot produce them.

### 2. The registry is the build authority

Submissions contain source, never binaries. CI compiles the artifact and signs it as the
`shilpo-rs/extensions` build authority. There is no author-held publisher key:

- `owners.toml` maps extension id namespaces to GitHub owners and holds no keys.
- The protected publication environment holds a package-signing key and an index-signing key.
- Manifest `authors` entries are attribution and ownership metadata, not cryptographic
  identities.

### 3. Publication runs in three trust stages

1. **PR validation** — no secrets, artifact is advisory and never published.
2. **Main-branch build** — no secrets; rebuilds the exact merged commit; emits the package,
   an *unsigned* canonical index payload, and provenance.
3. **Privileged publication** — protected environment; consumes the artifact bound to the
   exact merged SHA; executes no extension or repository build code; verifies provenance,
   signs the bytes it was handed, publishes the Release asset, and commits the index.

Two stages are insufficient: a PR artifact may originate from a synthetic merge commit, may
expire, may have been produced by a contributor-modified workflow, and the merged commit may
differ from what was validated. The privileged stage never runs the index generator while
holding a signing key. `pull_request_target` is forbidden in the registry repository, and
ruleset protection covers `.github/workflows/**`, `scripts/**`, `owners.toml` and `CODEOWNERS`.

### 4. Index integrity is Ed25519, with ordering

Signing stays Ed25519, matching what the client already verifies. Sigstore keyless signing is
attractive — it removes the long-lived secret and binds the index to a workflow identity — but
it does not fit the current wire format and is deferred to a spike.

The signed payload gains a monotonic counter. A lower counter is rejected; an equal counter
with an identical digest is a no-op; an equal counter with a differing payload is rejected. A
stale `generated_at` **warns** rather than rejects, because a valid static repository may
legitimately publish nothing for months; hard expiry, if ever wanted, is an explicit signed
`expires_at` the publisher opts into.

### 5. Sources are federated; added sources are the user's own risk

The official source ships enabled. Users may add any HTTPS index URL. A user-added source
supplies its own key from the same origin as its index, so its signature proves only that the
index came from whoever gave them the URL. The single mitigation is trust-on-first-use: the
key is pinned when the source is added and an unauthorised change is a hard failure with an
explicit recovery path. Beyond that, a user who adds an unofficial source owns that risk, and
no further trust machinery is built for that case.

The official public key and index URL are compiled into Shilpo as constants. A public
verification key is not a secret, and a distribution that forgets a build-time environment
variable must not silently ship a shell with no extension source at all. Environment overrides
remain for development and testing; private signing keys exist only in the protected
publication environment.

### 6. Installation identity stays global; provenance is pinned

There is one installed package per `ExtensionId`. The install receipt pins the source it came
from, the source key in effect at install time, and the package signer. Updates may only come
from that same source. Another source offering the same id is surfaced as a conflict and can
neither shadow nor update the installed extension. Switching an extension to a different source
is an explicit operation and does not carry grants or secrets across publishers.

Source ids are reserved: adding a source whose id is already taken is an error, not an
overwrite, since the on-disk index cache is addressed by source id.

### 7. "Official" is a per-extension property requiring two independent facts

An extension is official only when the source's key is one compiled into the Shilpo binary
**and** the release itself carries the official signal. A user-added source declaring itself
official in its own configuration never qualifies.

Registry CI authorises that signal only when the manifest `authors` list contains exactly the
canonical maintainer identity **and** the extension id's namespace is owned by that maintainer
in `owners.toml`. `org.shilpo.*` is reserved; community ids take the form
`io.github.<login>.<extension>`. Namespace ownership is recorded only when a maintainer-approved
pull request merges, so an open pull request cannot squat a namespace.

### 8. The wire contract is Core Tier

`RegistryIndex`, `RegistryRelease`, `SignedRegistryIndex`, `PackageSignature` and the signing
payload construction are cross-platform distribution contracts, not host-runtime internals.
They move out of `desktop/ext-runtime` into the Core Tier so the registry's index generator can
depend on them without pulling in the Wasmtime host runtime. See
[ADR-0001](0001-cross-platform-linux-split.md) for the tier rules.

The generator serialises those real types rather than emitting hand-written JSON. Every one of
them carries `#[serde(deny_unknown_fields)]`, so any drift between generator and client is a
hard install failure; making the generator use the same structs removes the drift class
entirely rather than testing for it. JSON Schema is emitted from the same types and published
at an immutable versioned path, with a build failure on divergence — the published schema
mirrors the Rust types and is not an independent source of truth.

## Consequences

- Distribution costs nothing to operate. There is no service to keep up, no bill, and no live
  attack surface on the path that delivers executable code — at the price of permanently
  forgoing install counts, popularity ranking and any server-side moderation signal.
- Users install bytes that were built from source a maintainer reviewed, rather than a binary a
  publisher uploaded. This is a stronger provenance guarantee than a publisher signature alone
  provides, and it makes every extension in the official registry necessarily open-source.
- Every published index is a reviewable git diff as well as a signed artifact, which gives a
  forensic record for a channel that distributes executable code.
- Compromise of the publication environment is compromise of index integrity. Ed25519 accepts
  that exposure deliberately; #299 tracks removing it.
- Because the artifact is CI-built, third-party sources cannot federate into the official
  registry's trust — they are separate sources with their own keys, and the client treats them
  accordingly.
- `discover()` currently matches `api_version` by exact equality, so the first WIT contract
  revision will hide extensions from shells that have not updated. With no users yet this is
  taken as a clean break; a compatibility ladder is tracked separately and must land before any
  release where mixed shell versions are expected.
- Follow-up tracker issues: #104 (publication pipeline), #295 (source-pinned provenance),
  #296 (official trust signal), #297 (replay protection and key pinning), #298 (registry
  eligibility), #299 (Sigstore spike), #300 (API version ladder), #301 (Core Tier wire
  contract), #302 (gallery website and install flow).
