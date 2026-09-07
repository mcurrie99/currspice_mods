# currspice_mods — Roadmap & Technical Direction

> **Purpose:** Source of truth for *how* this project evolves. Future work should match a phase below; if a new idea doesn't fit, add it to the right phase (or propose a new one here before building it).
> **How to use:** Phases are ordered by dependency. Each item has acceptance criteria. Update status markers (`⬜` todo / `🚧` in progress / `✅` done) as we go.
> **Maintenance note:** Read `README.md` for current runtime behavior and `docs/DEVELOPMENT.md` for verification evidence and the next bounded task. Historical completion notes below describe their stated checkpoints, not a fresh verification of your working tree.

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
| P1 | **Manifest is the contract.** The canonical catalog is `docs/mods.yaml`; a CWD `mods.yaml` or explicit file is a local override. | A compatible installer can consume a new external manifest without a code release. Updating an executable's embedded snapshot requires rebuilding it. |
| P2 | **Manifest resolution works offline.** Explicit/CWD manifest overrides plus an embedded default remove the manifest-host dependency. | A fresh installation still needs artifact downloads. Strict no-download mode must reject missing prerequisites. Existing unrelated files are preserved, so identical manifests do not imply identical whole-machine state. |
| P3 | **Idempotent installs.** Re-running must be safe and cheap (skip what's current, fix what's stale). | No "I already ran it" support tickets. |
| P4 | **Rust-core app.** Rust end-to-end where it counts (installer, axum backend, Tauri shell); the app UI is a lightweight web view inside Tauri. No Node build toolchains, no heavy frontend frameworks. | One toolchain, one CI, small attack surface. |
| P5 | **Distribution ≠ control.** GitHub Releases are the planned distribution channel; the future backend handles interactive control. | Installation must not require backend login or server-control availability. Joining multiplayer obviously still requires the Minecraft server. Release publication is separate from the current CI artifact upload. |
| P6 | **YAML stays human-editable.** It's the thing you'll edit at 1am to bump a mod. | No JSON-schema tooling required to publish a change. |
| P7 | **Fail loud, fail early, fail with a sentence a 16-year-old understands.** | Installer console output is its UI; app errors are human too. |
| P8 | **YAGNI.** No multi-server, no self-signup, no mod marketplace until the current shape is boring. (Accounts *are* in scope — user-approved.) | Phases below stay small. |

---

## Phase 0 — Foundations & Correctness (do first; small, high-value)

Fix the current shape before adding anything.

- [x] **Resolve manifest drift.** (DONE 2026-08-30 — `docs/mods.yaml` restored to `main`'s working `1.21.11`; root `mods.yaml` gitignored as a runtime artifact; added `Config::verify_version_consistency`, a whole-token drift guard that hard-stops a 1.21.1-declared / 1.21.11-built manifest with a human message.) Verified: the real 1.21.11 manifest passes; the `1_21_x_update` manifest is rejected at exit 1 naming every bad URL.
- [x] **Restore Fabric installer arguments.** (DONE — `Fabric::run_installer` now runs the verified headless `java -jar … client -dir … -mcversion … -loader …`, seeds `launcher_profiles.json` if absent, and *verifies* the `fabric-loader-<v>` profile actually appeared. `mc_version`/`inst_version` are used again. Note: the maven `.exe` is a GUI app that can't run unattended — we execute the `.jar`; both commands are locked by tests.) Reproduced locally against a real 1.21.11 / 0.18.4 install.
- [x] **Make profile editing robust.** (DONE — `build_java_args` splits on whitespace, never `-`; Xmx present/absent/empty and dash-heavy flags all covered by tests; a missing profile is now a clear error instead of a corrupted write.)
- [x] **Wire up or kill `user_mods`.** (DONE — `Config::all_mods()` yields server + user mods and the installer loops both, idempotent by presence.)
- [x] **CLI surface via `clap`.** (DONE — `--mc-dir`, `--config`, `--dry-run`, `--verbose`; `--help` is self-contained.)
- [x] **Verify on completion.** (DONE — installer prints an installed / already-present summary plus the resolved mods path and heap; downloads go to `.part` files and stale ones are cleaned, so no half-jar.)
- [x] **Windows build from anywhere.** (DONE — CI now has a `windows-latest` job: `cargo build --release` + full test suite + package `currspice_mods-windows-x86_64.exe` as an artifact. Chosen over winres/MSVC cross-compiling because it needs no fragile toolchain.)

## Phase 1 — Reliable Self-Contained Installer

- [ ] **Embedded default manifest.** `include_str!` a tracked default into the binary. Precedence: `--config` > `./mods.yaml` (CWD) > embedded. Removes the manifest-host dependency. Missing Fabric/Minecraft/mod artifacts and possibly the private installer JRE still require network access; an empty machine is not offline-installable.
- [ ] **Checksums in the manifest & enforced at install.** Add `sha256` per mod entry; download to `.part` temp file, verify hash, then atomic rename. On mismatch: delete, retry once, then fail with the hash in the message.
  - *Done when:* corrupted-partial-download scenario (simulate by writing a truncated file) ends in a clean retry-or-fail, never a half-jar left in `mods/`.
- [ ] **Idempotency.** If target exists and sha256 matches: `skipped`. Exists but differs: `replaced`. Missing: `installed`. Same summary output as P0 verification. No re-download on re-run when current.
- [ ] **Structured exit codes + `--verbose`.** 0 ok · 1 config · 2 network · 3 install-run · 4 verify. Verbose logs URLs/hashes and raw commands; default stays human sentences (P7).
- [ ] **Sequencing safety.** Fabric install → *verify Fabric profile exists* → edit profile → mods. Keep the dependency explicit with one clear error. The Phase 0 implementation already short-circuited failures; Phase 1 must cover this sequencing with deterministic pipeline tests.
- [ ] **Smoke test binary** (or a `--selftest` flag, or an equivalent integration test target): runs the production pipeline against temporary test fixtures with stubbed HTTP and a fake Fabric runner, through `cargo test` in CI. Historical `testing/mods.yaml` is a legacy fixture, not the live catalog.
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
| Manifest host | GitHub Pages via `docs/` (`server.currspice.com`) | Existing publishing arrangement; Git provides history. The live URL is mutable and availability must be checked separately when distributing an update. |
| Distribution | GitHub Releases for the installer exe **and** the Tauri app binary | Deterministic, no server dependency (P5). |
| Fabric install | Headless Fabric installer **JAR**, invoked with a discovered or privately provisioned Java runtime | The upstream Fabric exe is a GUI program. A private JRE removes the need for friends to install Java themselves for this step; Minecraft game-runtime provisioning remains the official launcher's job. |
| Language | Rust for everything, incl. the app (Tauri shell + axum backend) | One toolchain; installer already proves the stack. |
| GUI | **Tauri** for the Currspice Home app — *user-approved direction (2026-08), supersedes the earlier "no GUI" decision* | The app is the home of the server for everyone; Tauri keeps a small Rust core + tiny binary; no Electron. The installer stays a console exe forever. |
| Auth | Real login: username + password (argon2) with roles `owner/admin/friend`; owner-provisioned invites, no self-signup | *User requirement (2026-08):* login system for the app; control & admin tools need an audit trail. |
| Admin tools | In scope: on-demand backup, backup history/restore, RCON, scheduled maintenance, roles | *User requirement (2026-08):* "telling the server to back up and things like that." |
| Manifest format | YAML, versioned, human-editable | It's the 1am edit; JSON tooling not required (P6). |
| State DB | SQLite on the box (accounts, roles, audit log, backup history); flat files elsewhere | The login system + audit needs real storage; SQLite is the boring choice. |
| Windows builds | Native `windows-latest` GitHub Actions runner for the current installer; Ubuntu job for portable tests | Avoids MSVC/resource cross-compilation fragility. Building and releasing the future Tauri app/backend is not implemented yet. |

## Open Questions (answer before their phase starts)

1. **Manifest target:** `docs/mods.yaml` currently declares Minecraft **1.21.11** / Fabric loader **0.18.4**. This resolves the repository drift question, not a live check of the server. Confirm the actual server matches before distributing an installer.
2. **Where do friends get the exe today** (Discord link? this machine? a shared folder?) — still requires the owner's answer. CI uploads a workflow artifact; that is not automatic GitHub Release publication.
3. **Is the Minecraft server on the same Linux box** the backend will live on? Which implementation (Paper/Forge/vanilla), and is RCON enabled?
4. **Role split:** who should be `admin` vs. plain `friend`? (drives Phase 3 provisioning + Phase 4 permissions)
5. ~~Trusted-friends-only assumption / "no real auth"~~ — **RESOLVED 2026-08-30:** real login (username + password, roles) required from app v1; the installer stays anonymous.
6. **What does a backup look like on the box today** (script name? command? world size? offsite copy?) — Phase 4's "Back up now" must call the *real* mechanism, not invent one.

## Status

| Phase | Status |
|-------|--------|
| 0 — Foundations | ✅ done 2026-08-30 (build + `cargo test` 39/39 release, good manifest passes, bad one hard-stops) |
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
