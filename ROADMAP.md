# currspice_mods — Roadmap & Technical Direction

> **Purpose:** Source of truth for *how* this project evolves. Future work should match a phase below; if a new idea doesn't fit, add it to the right phase (or propose a new one here before building it).
> **How to use:** Phases are ordered by dependency. Each item has acceptance criteria. Update status markers (`⬜` todo / `🚧` in progress / `✅` done) as we go.
> **Last updated:** 2026-08-30 (v2 — Tauri app, login system, and admin tools added per user direction)

---

## North Star

A friend should be able to go from "never played Currspice" to "in the game, mods working, right RAM" with **one download and one double-click** — no setup wiki, no Java fiddling, no Discord file archaeology.

On top of that: **the server has a home.** **Currspice Home** — a Tauri desktop app — is the front door to the server: friends log in, pull down their mod setup, see server status, and (where their role allows) start/stop the server, trigger backups, and check logs. You, the host, control the fleet — mod list, server lifecycle, backups, and who can do what.

Three pieces, one repo:

1. **The installer** (today's exe) — dumb, portable, deterministic, zero server dependency. *Reliability beats features here.*
2. **Currspice Home** (Tauri desktop app) — the home of the server for everyone: login, mod setup, status, controls, and admin tools where the role allows.
3. **The backend** (axum on the Linux box) — the app's brain: accounts/roles, server control, backups, audit. The app is the *only* interface; there is no browser dashboard.

The installer must keep working when the app/server is down (P5). The app must make the installer *better*, not be required by it.

---

## Design Principles (decide once, follow always)

| # | Principle | Consequence |
|---|-----------|-------------|
| P1 | **Manifest is the contract.** All mod knowledge lives in `mods.yaml`, never in code. | Changing the mod list must never require a code release. |
| P2 | **Installer is offline-capable & deterministic.** Embedded default manifest + optional local `mods.yaml`; same bytes in, same machine state out. | Testable, works on flaky connections, no hidden network deps. |
| P3 | **Idempotent installs.** Re-running must be safe and cheap (skip what's current, fix what's stale). | No "I already ran it" support tickets. |
| P4 | **Rust-core app.** Rust end-to-end where it counts (installer, axum backend, Tauri shell); the app UI is a lightweight web view inside Tauri. No Node build toolchains, no heavy frontend frameworks. | One toolchain, one CI, small attack surface. |
| P5 | **Distribution ≠ control.** The exe ships via GitHub Releases (static, immutable). The live server does only interactive things. | Friends never depend on the server being up to *play*. |
| P6 | **YAML stays human-editable.** It's the thing you'll edit at 1am to bump a mod. | No JSON-schema tooling required to publish a change. |
| P7 | **Fail loud, fail early, fail with a sentence a 16-year-old understands.** | Installer console output is its UI; app errors are human too. |
| P8 | **YAGNI.** No multi-server, no self-signup, no mod marketplace until the current shape is boring. (Accounts *are* in scope — user-approved.) | Phases below stay small. |

---

## Phase 0 — Foundations & Correctness (do first; small, high-value)

Fix the current shape before adding anything.

- [ ] **Resolve manifest drift.** `docs/mods.yaml` says `mc_version: 1.21.1` but its mod URLs are 1.21.11 builds; the untracked root `mods.yaml` says `1.21.11`. Decide the real version, make `docs/mods.yaml` the single authoritative copy (it's what GitHub Pages serves as `server.currspice.com`), and add root `mods.yaml` to `.gitignore` (it's a runtime artifact of `download_new`).
  - *Done when:* one source of truth, no stale copy in git, CI would catch a version mismatch between `mc_version` and mod URL slugs (simple test: every `server_mods` URL contains the `mc_version` string).
- [ ] **Restore Fabric installer arguments.** `Fabric::run_installer` (`src/mods.rs:131`) currently launches the exe with *no* arguments; the `java -jar ... client -mcversion ... -loader ...` path is commented out and `mc_version`/`inst_version` are ignored. The commented code was also `java -jar` (the new artifact is a `.exe`). Re-enable an explicit, correct invocation (exe + `client -mcversion <v> -loader <v>`) or delete the unused fields.
  - *Done when:* a fresh install on a clean machine produces the exact requested MC version + loader, verified by the profile name in `launcher_profiles.json`.
- [ ] **Make profile editing robust.** `Config::edit_profile` (`src/config.rs:126`) splits `javaArgs` on `-` — brittle (breaks on any argument with an inner `-`, and the `Xmx` token search is fragile). Replace with whitespace-based argument parsing.
  - *Done when:* unit test covers "Xmx present", "Xmx absent", "no javaArgs" (fallback to `JVM_ARGS`), and an argument containing a dash.
- [ ] **Wire up or kill `user_mods`.** Config parses it, nothing installs it. Either the installer loops it (with the same idempotency guarantees as server_mods) or the field goes away.
- [ ] **CLI surface via `clap`.** Add `--mc-dir <path>` (kills the hardcoded `C:\Users\%USERNAME%...` TODO in `main.rs:14`), `--config <path>`, and `--dry-run`.
  - *Done when:* `currspice_mods.exe --help` is self-explanatory; non-default install dirs work.
- [ ] **Verify on completion.** After installing, re-read the mods folder, count files vs. manifest entries, print a summary table (`name — ok/new/skipped`).
- [ ] **Windows build from anywhere.** The `cross`-compile pipeline commit exists but CI (`rust.yml`) only builds+tests on Ubuntu. Add a job: cross-compile the Windows MSVC exe (e.g. `cargo xwin` or `cross --target x86_64-pc-windows-msvc`) and attach the exe as a **GitHub Release artifact**.
  - *Done when:* a PR to `main` produces `currspice_mods.exe` in a draft release that runs on Windows.

## Phase 1 — Reliable Self-Contained Installer

- [ ] **Embedded default manifest.** `include_str!` a tracked default into the binary. Precedence: `--config` > `./mods.yaml` (CWD) > embedded. Lets you ship a release that's *always* installable, even with zero network to the manifest host (mod CDNs obviously still needed).
- [ ] **Checksums in the manifest & enforced at install.** Add `sha256` per mod entry; download to `.part` temp file, verify hash, then atomic rename. On mismatch: delete, retry once, then fail with the hash in the message.
  - *Done when:* corrupted-partial-download scenario (simulate by writing a truncated file) ends in a clean retry-or-fail, never a half-jar left in `mods/`.
- [ ] **Idempotency.** If target exists and sha256 matches: `skipped`. Exists but differs: `replaced`. Missing: `installed`. Same summary output as P0 verification. No re-download on re-run when current.
- [ ] **Structured exit codes + `--verbose`.** 0 ok · 1 config · 2 network · 3 install-run · 4 verify. Verbose logs URLs/hashes; default stays human sentences (P7).
- [ ] **Sequencing safety.** Fabric install → *verify Fabric profile exists* → edit profile → mods. Today a failed Fabric install still attempts profile edits with a confusing error. Make the dependency explicit with one clear error.
- [ ] **Smoke test binary** (or a `--selftest` flag): runs the whole pipeline against `testing/mods.yaml` with stubbed HTTP, in CI, in the dev directory.
  - *Done when:* `cargo test` stays green on Ubuntu *and* a manual Windows pass is a 3-minute checklist in the README.

## Phase 2 — Manifest v2 (the contract evolves)

Schema is still YAML (P6) but gains structure. Old files must still load with a warning.

```yaml
v: 2
mc:
  version: "1.21.11"
  loader: "0.18.4"
  jvm_args: "-Xmx4G -XX:+UseG1GC ..."   # replaces the crate constant
mods:
  - id: sodium                      # stable slug you can reference
    name: Sodium
    source: { type: modrinth, slug: AANobbMI }   # installer composes CDN URL, or…
    url: "https://cdn.modrinth.com/..."          # …explicit override
    sha256: <hex>
    required: true
  - id: dhsoundpack
    name: DH Soundpack
    source: { type: url, url: "https://..." }
    required: false              # friends opt in
```

- [ ] `v:` field + forward-compat: installer rejects a `v` it doesn't know with a human error ("installer too old — download the new one").
- [ ] Structured entries above (id/source/sha256/required). Keep raw `url` as the escape hatch for non-Modrinth mods.
- [ ] **Optional mods path in installer:** `--optional all|none|id1,id2`. Default installs `required: true` only.
- [ ] `jvm_args` from manifest wins over the compiled-in constant (keeps JVM tuning a *data* change).
- [ ] **Personal manifest endpoint** (prep for the app, but usable standalone): `GET /manifest?user=<id>&optional=...` on the backend serves a friend's selected modset as an installer-consumable `mods.yaml` — so "copy my install steps" is just one URL.
- [ ] Publishing doc: one page on the GitHub Pages site (`docs/`) — "how to add a mod", "how to bump version", with the sha256 gotcha.

## Phase 3 — Currspice Home (Tauri app) + Axum backend + Login

Your `IDEA.md` ambition, now the approved direction (user decision 2026-08): **a Tauri desktop app that is the home of the server.** New crates in the same workspace:

- `api/` — **axum** backend on the Linux box (systemd): accounts, roles, server control, backups, audit.
- `home/` — **Tauri** app (Windows binary, CI-produced): thin, polished client over the axum API.

- [ ] **Workspace layout:** `installer/` (current code), `api/` (axum), `home/` (Tauri), `manifest/` (shared types). One CI builds all; one tag publishes installer exe + home exe + api binary.
- [ ] **Login system (REQUIRED by user; app is unusable past the welcome screen without one):** real accounts — owner (you), admins, friends. Username + password (argon2, backend-side), session tokens on the client. First-run: you pre-provision friends from the owner UI (name + role + one-time invite token); they redeem it at first login. **No public self-signup** (trusted friends only — P8).
  - *Done when:* a friend's first launch is a clean "enter your invite" flow; wrong password fails with a human message; every state-changing API call is role-checked; every one lands in an audit log (who/what/when) that the owner can read in-app.
- [ ] **Roles:** `owner` (you — everything, incl. account management and restore) / `admin` (server control, backups) / `friend` (read status, own mod setup, proposals). Document the permission matrix in-app ("what you can do") and in this file.
- [ ] **Mod catalog screen:** shows `required` + `optional` mods with your notes; a friend toggles optionals and gets their **personal install path** — the app composes the Phase 2 personal-manifest URL and/or an installer command. One click to "copy my install steps".
- [ ] **Status screen:** server up/down (process check), version, online player count (RCON `list` if RCON is on; else just up/down), last backup time (Phase 4).
- [ ] **Server control (admin+, role-gated):** `start` / `stop` / `restart` via the box's systemd unit, audited with timestamp+who.
- [ ] **Log peek:** last N lines of the server log, refreshable (friend+).
- [ ] **Latest-installer badge:** shows the newest GitHub Release version + "download latest" link (Phase 2 endpoint, Phase 4 automation).
- [ ] **Proposals queue (the "share mods with one another" goal):** a friend pastes a Modrinth link + one-line why from the app → lands in a queue → owner approves → scripted commit into `docs/mods.yaml` (not by hand) → everyone sees it at next install. *This is the loop that replaces Discord mod-pinning.*

## Phase 4 — Admin Tools & Server Ops (after Phase 3 is boring)

User-approved scope (2026-08): *"admin tools like telling the server to back up and things like that."*

- [ ] **Backup on demand (admin+):** "Back up now" button → api runs the box's real backup job (systemd oneshot or script — see open Q6), streams start/progress/done back to the app, records last-backup timestamp + size.
  - *Done when:* a click produces a finished backup, its size/size/time visible in the app, and the same entry appears in history.
- [ ] **Backup history & restore (owner):** list of backups (name, date, size, world); restore behind a two-step in-app confirmation ("type the world name") + audit line. *Constraint: restore only automates a restore path that already exists and works on the box — the app must not invent a recovery mechanism.*
- [ ] **World/server ops (admin+):** graceful stop with broadcast warning (RCON `say` if enabled), scheduled maintenance windows (e.g. nightly restart) shown on the status screen, pause/resume.
- [ ] **RCON integration:** `say`, `broadcast`, `kick`, friend whitelist — surfaced in-app, role-gated.
- [ ] **Auto-update nudge:** installer (networked runs) compares manifest version vs. its local summary file, prints one line "new modset available (v12 → v14)" — never forced, never silent. The app shows the same as a badge.
- [ ] **Release automation:** merge to `main` → build installer + home (Windows) + api → tag → GitHub Release → app badge + installer nudge update. The whole "update the server" workflow becomes: edit `docs/mods.yaml`, push, done.
- [ ] (Maybe) per-friend install telemetry — anonymous "v14 installed, 3 mods, 1 failed" counter on the box. Useful for support, optional for privacy.

---

## Decisions Log

| Area | Decision | Why |
|------|----------|-----|
| Manifest host | GitHub Pages via `docs/` (`server.currspice.com`) | Already working; free; immutable history. |
| Distribution | GitHub Releases for the installer exe **and** the Tauri app binary | Deterministic, no server dependency (P5). |
| Fabric install | Windows exe path (no Java prerequisite) | Matches "Transitioned to .exe fabric install" commit; friends don't have Java. |
| Language | Rust for everything, incl. the app (Tauri shell + axum backend) | One toolchain; installer already proves the stack. |
| GUI | **Tauri** for the Currspice Home app — *user-approved direction (2026-08), supersedes the earlier "no GUI" decision* | The app is the home of the server for everyone; Tauri keeps a small Rust core + tiny binary; no Electron. The installer stays a console exe forever. |
| Auth | Real login: username + password (argon2) with roles `owner/admin/friend`; owner-provisioned invites, no self-signup | *User requirement (2026-08):* login system for the app; control & admin tools need an audit trail. |
| Admin tools | In scope: on-demand backup, backup history/restore, RCON, scheduled maintenance, roles | *User requirement (2026-08):* "telling the server to back up and things like that." |
| Manifest format | YAML, versioned, human-editable | It's the 1am edit; JSON tooling not required (P6). |
| State DB | SQLite on the box (accounts, roles, audit log, backup history); flat files elsewhere | The login system + audit needs real storage; SQLite is the boring choice. |
| Cross-compile | `cargo xwin`/`cross` in CI for Windows MSVC installer + Tauri Windows app | You compile anywhere; friends always get Windows artifacts. |

## Open Questions (answer before their phase starts)

1. **What's the real target MC version right now?** 1.21.1 or 1.21.11? (Phase 0 drift)
2. **Where do friends get the exe today** (Discord link? this machine? a shared folder?) — determines how urgent "GitHub Releases" is.
3. **Is the Minecraft server on the same Linux box** the backend will live on? Which implementation (Paper/Forge/vanilla), and is RCON enabled?
4. **Role split:** who should be `admin` vs. plain `friend`? (drives Phase 3 provisioning + Phase 4 permissions)
5. ~~Trusted-friends-only assumption / "no real auth"~~ — **RESOLVED 2026-08-30:** real login (username + password, roles) required from app v1; the installer stays anonymous.
6. **What does a backup look like on the box today** (script name? command? world size? offsite copy?) — Phase 4's "Back up now" must call the *real* mechanism, not invent one.

## Status

| Phase | Status |
|-------|--------|
| 0 — Foundations | ⬜ |
| 1 — Reliable installer | ⬜ |
| 2 — Manifest v2 | ⬜ |
| 3 — Currspice Home app (Tauri + axum + login) | ⬜ |
| 4 — Admin tools & server ops | ⬜ |

## Backlog (unsorted ideas — pull up into a phase when they're real)

- Mod "pin" UI: friends vote on which optional mods deserve required status.
- Server-side mod mirror (proxy Modrinth CDN) — only if a friend's network blocks CDNs.
- One-liner PowerShell/bat wrapper so double-click is zero-console.
- Per-friend "last played" presence on the status screen.
- Multi-server support (second SMP?) — blocked by P8 until it actually happens.
