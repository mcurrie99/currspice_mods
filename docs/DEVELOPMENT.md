# Development and coding-agent handoff

Read this with [`../AGENTS.md`](../AGENTS.md), [`../README.md`](../README.md),
and [`../ROADMAP.md`](../ROADMAP.md). This document is deliberately explicit so
an agent starting with an empty context can continue without guessing.

## Scope and state

The implemented product is the standalone Windows console installer. Currspice
Home, axum, accounts, invitations, roles, server control, backup/restore, and a
shared multi-crate workspace are approved **future work**, not current features.

The source manifest targets Minecraft 1.21.11 / Fabric loader 0.18.4. This records
repository data only: neither the server version nor mod compatibility in a
running game is established by parsing YAML or passing unit tests.

The current repair task completes the interrupted Phase 1 manifest/checksum
work and its installer safety guarantees. Do not use it as authorization to
start Phase 2 or reorganize the repository into a workspace.

## Module map

| Path | Responsibility |
|---|---|
| `src/main.rs` | Parse CLI arguments, resolve manifest and Minecraft path, report exit status |
| `src/config.rs` | Legacy/structured manifest types, validation/selection, version drift checks, launcher-profile edits |
| `src/download.rs` | Typed installer errors, injectable HTTP transport, streamed file hashing |
| `src/mods.rs` | Safe artifact names, verified mod installation and outcomes, Fabric command/profile contract |
| `src/pipeline.rs` | Production orchestration and preflight, with injected transport/runtime for tests |
| `src/jre.rs` | Locate/probe installer Java, provision the private runtime, validate/extract archives |
| `src/tools.rs` | Windows Minecraft directory discovery and RAM sizing |
| `src/lib.rs` | Public modules and compiled branding/default constants |
| `docs/mods.yaml` | Canonical published and embedded manifest |
| `scripts/gen_manifest.py` | Explicit maintainer operation for manifest validation/hash generation |
| `tests/` | Integration regressions for manifest selection, downloads, Fabric, pipeline, and CLI |
| `.github/workflows/rust.yml` | Ubuntu/Windows checks and Windows executable artifact |

Follow data through these layers when debugging. For example, adding a hash
field to `Config` is insufficient unless `pipeline` passes it into the verified
mod installer. A helper being unit-tested does not prove the CLI actually uses it.

## Invariants to preserve

### Manifest and compatibility

- `docs/mods.yaml` is the one canonical catalog, published by GitHub Pages.
- A root `mods.yaml` is a local runtime override. Never commit it or silently
  overwrite it after a parsing error.
- Explicit `--config` must take precedence; a missing/invalid explicit file is an
  error. An existing invalid CWD override is also an error, not permission to
  download something different.
- The installer must have an embedded default so resolving the manifest does not
  require the future backend, GitHub Pages, or network access.
- Legacy string URLs remain readable. Structured entries use `url` and optional
  `sha256` keys. This transitional format is **not** the future Phase 2 schema.
- Verify hashes when supplied. Do not reinterpret an invalid digest as absent.
- Validate new data before mutation. Source URLs require HTTPS in production;
  use dependency injection to test downloads without weakening that contract.

### Downloads and installation

- Inspect HTTP status before installing the response body. A 404 HTML page is
  not a jar merely because it was fetched from a URL ending in `.jar`.
- Stage content away from the final filename, verify it, then publish it. On
  mismatch, preserve any previous target and clean temporary files.
- A file's existence does not prove integrity. With a manifest checksum, skip
  only matching bytes; replace mismatching bytes. With a legacy URL-only entry,
  explain the weaker presence-only behavior rather than claiming verification.
- Do not remove unrelated mods. Upgrading a URL to a different filename may
  leave an older jar behind; a managed-file inventory/removal policy is separate
  work, not permission to clear a user's mods directory.
- Fabric runs as a JAR through Java, not by invoking a GUI exe unattended.
- Fabric execution must succeed and the expected JSON launcher-profile entry
  must exist before profile edits and mod installation.
- Preserve other launcher profiles, settings, and unrelated files.
- Private Java is for executing the Fabric installer. Do not point a modern
  Minecraft game at that JRE or claim the installer provisions the game runtime.
- Reject archive path traversal when extracting a downloaded JRE.

### Command-line contract

- `--dry-run` resolves and validates inputs, reports intent, and makes no
  filesystem changes, network requests, or Fabric/Java-install calls.
- `--no-download` must not indirectly launch Fabric when that process could
  download dependencies. Detect missing prerequisites before making changes.
- Default output should explain errors in ordinary sentences. Verbose output
  should add useful URLs, hashes, and commands rather than only suppressing a
  suggestion to rerun verbosely.
- Exit status is part of the API. Keep documented values and tests aligned.

## Testing strategy

Use a small failing regression test for each bug. The full suite is a final
regression gate, not a substitute for demonstrating that a focused test catches
the original failure. Compiler errors and assertion failures are different
signals: explain which one a command produced.

Use temporary directories for launcher profiles, mods, manifests, and Java
archives. The pipeline smoke test must exercise the production orchestration
with an injected transport and Fabric runner. It must not contact Modrinth,
Adoptium, Fabric, or Mojang during `cargo test`.

Essential behavior matrix:

| Area | Required cases |
|---|---|
| Manifest | Legacy URL; structured entry; mixed lists; YAML roundtrip; null/empty optional list; malformed hash; missing/invalid explicit override; CWD precedence; embedded fallback |
| Integrity | Missing mod installs; matching hash skips; corrupt target replaces; bad response retries once; second mismatch fails without publishing; previous target survives failure |
| HTTP | Success; non-success status; transport failure; no error body installed; useful failure classification |
| Files | Unsafe filename/archive path rejected; temporary file cleanup; unrelated target content preserved |
| Pipeline | Fresh install; rerun; corrupt mod repair; Fabric failure prevents profile edits and mod install; missing profile rejected |
| CLI | Dry-run is read-only; no-download makes zero transport/Fabric-download calls; explicit config error is not hidden; verbose output useful; documented exit statuses |
| Generator | Offline syntax/schema check; valid mapping output; hash association preserved; user mods not silently dropped; generation roundtrip |

The table is an acceptance checklist, not a claim that every possible failure is
covered. See the named tests and verification record below for actual evidence.

## Recommended next work, in dependency order

### 1. Close release-validation gates

- Run GitHub Actions on the proposed branch through a PR targeting `main` (after
  the owner authorizes pushing). Confirm the actual Ubuntu and Windows jobs,
  not just the presence of `.github/workflows/rust.yml`.
- Run the README manual Windows checklist on a disposable test installation.
- Confirm the live server version and that the bundled mod catalog launches and
  joins it successfully. CDN availability alone is not mod compatibility.
- Review the diff and decide how the first repaired installer is distributed.
  A workflow artifact is not a public GitHub Release or an auto-update system.

Acceptance: actual CI links and manual outcomes are recorded. If external
services block testing, record that blocker rather than marking the gate green.

### 2. Start Phase 2 with one parser compatibility slice

Only after Phase 1's release gates are addressed, the next implementation task
should be versioned manifest parsing, not the UI or API. Read the full Phase 2
schema in ROADMAP before choosing public types.

Suggested bounded task:

1. Add a failing test for a declared unsupported `v` producing a clear
   "installer too old" error.
2. Define explicit legacy and v2 parsing/validation behavior, including missing
   `v`, warnings, and preserving the transitional checksum format.
3. Implement the smallest version dispatch and test old manifests still load.
4. Document behavior and update only the acceptance criteria actually met.

Then add structured IDs/required flags, optional selection, and manifest JVM
arguments as separate tested slices. The personal-manifest endpoint depends on
an actual backend; do not invent one while implementing the parser.

### 3. Gather owner answers before Phase 3/4

- Where will the Linux backend run relative to the Minecraft server?
- What is the server implementation/systemd unit, and is RCON enabled?
- Which friends are admins, and how will owner credentials/invites be provisioned?
- What exact backup command and tested restore procedure exist today?
- Where are backups stored and how is offsite retention handled?

Do not guess these values or implement backup/restore against a fictional host.

## Verification record

This section is updated after the implementation and independent checks finish.
Pending evidence must not be treated as passed. Historical Phase 0's 39-test
record in ROADMAP is not a count for the current source.
