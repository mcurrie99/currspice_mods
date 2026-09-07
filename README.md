# Currspice Mods

A **Windows x64 console installer** for the Currspice Minecraft SMP. It installs
Fabric, configures a branded launcher profile and RAM allocation, and installs
the mod catalog from a YAML manifest.

**Current product:** the installer only. **Future product:** Currspice Home
(Tauri), an axum backend, login/roles, and server administration. Those future
features are approved in [ROADMAP.md](ROADMAP.md), not implemented here.

Coding agents: start with [AGENTS.md](AGENTS.md) and the
[development handoff](docs/DEVELOPMENT.md).

## Before running

- Use Windows x64 with the official Minecraft launcher installed. Sign in and
  launch vanilla Minecraft once so its installation directory exists, then close
  the game before changing mods or launcher profiles.
- The current canonical manifest targets **Minecraft 1.21.11** and **Fabric
  loader 0.18.4**. Confirm these match the server before distributing a build.
- A normal first install needs internet for missing Fabric/Minecraft dependencies,
  mod jars, and possibly a private Java runtime. An embedded manifest removes the
  manifest-host dependency; it does **not** bundle every jar or make a fresh
  installation completely offline.
- Back up your launcher profiles and mods before a real installation. For release
  testing use a disposable Minecraft directory, not your daily installation.
- This tool does not download the official launcher, provision a Minecraft
  account, or install the future Currspice Home app.

The private Java runtime is for the **Fabric installer**. Minecraft itself uses
the runtime selected/provisioned by the official launcher; a Java 17 installer
runtime is not a promise that Java 17 can run this Minecraft version.

## Build and run

Install stable Rust with the Windows MSVC toolchain and the Visual Studio C++
build tools/Windows SDK. Resource compilation embeds `icon.ico`. The project uses
Rust edition 2024; the repair was tested with Rust/Cargo 1.90.0, not every older
edition-compatible release.

From the repository root:

```bash
cargo build --locked --release
./target/release/currspice_mods.exe --help
```

On this maintainer's Hermes desktop host, the shell is Git Bash and Cargo may not
be on PATH. Use:

```bash
export PATH="/c/Users/matth/.cargo/bin:$PATH"
```

or invoke `"C:/Users/matth/.cargo/bin/cargo.exe"` directly. Do not use that
maintainer-specific path on other machines or put it in application source.

### Normal installation

Double-click a built Windows executable, or run it from a terminal. The automatic
Minecraft directory detection is Windows-specific; use `--mc-dir` for a custom
location. The target directory must already exist.

```bash
./target/release/currspice_mods.exe --mc-dir "C:/Users/YOUR_NAME/AppData/Roaming/.minecraft"
```

A custom manifest:

```bash
./target/release/currspice_mods.exe --config "C:/Currspice/my-mods.yaml" --mc-dir "C:/Currspice/test-minecraft"
```

Replace example paths with real ones. These are Git Bash examples; in PowerShell
use `.\target\release\currspice_mods.exe` for the executable path.

### Preview safely

```bash
./target/release/currspice_mods.exe --dry-run --config docs/mods.yaml --mc-dir "C:/Currspice/test-minecraft"
```

`--dry-run` validates and reports what would be done without downloading, writing
files, or running Fabric. It does not test CDN availability or whether the game
will launch. `--verbose` adds diagnostic detail for a real run.

`--no-download` is a strict network prohibition, not a request to ignore failures.
It cannot turn a fresh empty directory into a complete offline Minecraft install.
The exact preflight requirements and final verification results are documented
in the development handoff after the implementation checks finish.

## Manifest contract

`docs/mods.yaml` is the canonical mod catalog and GitHub Pages source. A root
`mods.yaml` is an ignored, local override; do not commit it.

Resolution order:

1. `--config PATH`, when supplied.
2. `mods.yaml` in the process's **current working directory**, not necessarily
   beside the executable.
3. The canonical manifest embedded when the executable was built.

A missing or invalid explicit file is an error. An invalid existing CWD manifest
is also an error. Neither is silently replaced by a network download. To use the
embedded default, run without `--config` in a directory with no `mods.yaml`.

This means an old executable's embedded catalog does not automatically change
when GitHub Pages changes. Obtain the current manifest and pass it explicitly,
or distribute a newly built executable. There is no silent auto-update.

### Transitional schema (before Phase 2)

```yaml
path: ""                       # resolved/overridden by the CLI at runtime
mc_version: "1.21.11"
installer_version: "0.18.4"     # Fabric LOADER version, despite the legacy name
fabric: "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar"
server_mods:
  - url: "https://example.org/example-mod-1.21.11.jar"
    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
user_mods: []
```

The example mod URL and digest are **illustrative, not installable catalog data**.
A structured item starts with `- url:`, not a bare URL with an indented hash.
SHA-256 values must be actual 64-character hexadecimal digests of the desired
jar bytes.

Legacy entries such as `- https://example.org/mod.jar` remain supported. Without
a checksum, a pre-existing file can only be skipped by presence; the installer
cannot establish its integrity. Both `server_mods` and `user_mods` are installed;
`user_mods` is **not yet an opt-in UI or `--optional` selection feature**.

HTTPS and manifest consistency checks help catch mistakes; URL-based version
checks do not prove that the complete mod combination is compatible. A checksum
validates bytes against your manifest, not the trustworthiness of a mod author
or a compromised manifest source.

## Maintaining the canonical manifest

The maintenance script is Python tooling for the owner, not a runtime dependency
of the installer. Install its pinned dependency:

```bash
python -m pip install -r scripts/requirements.txt
```

The intended maintenance sequence is:

1. Edit `docs/mods.yaml` with a trusted direct HTTPS jar URL for the correct
   Minecraft/Fabric version. Preserve existing hashes for unchanged URLs.
2. When intentionally changing an entry's URL, remove that entry's old hash so
   the maintenance command can record the newly selected bytes. Do not keep a
   previous version's digest attached to a new URL.
3. Run `python scripts/gen_manifest.py` to calculate **missing** hashes. This is
   a network/write operation; existing valid hashes are retained, not rechecked.
   Run `python scripts/gen_manifest.py --refresh` only when deliberately
   re-downloading and re-hashing **all** entries, then inspect every change.
4. Run the offline validation and tests described below. Review URL/digest pairs
   in `git diff -- docs/mods.yaml` and verify game compatibility separately.
5. Commit the canonical manifest only after review. Rebuild for a new embedded
   snapshot, or distribute the updated manifest explicitly alongside an older
   compatible installer.

The interrupted `defaults/mods.yaml` file was a duplicate, malformed attempt at
an embedded catalog. Its recorded URL/hash pairs are being incorporated into
`docs/mods.yaml`; do not reintroduce a separately maintained default catalog.

## Troubleshooting and exit codes

Run from a terminal when collecting errors so the window stays available. Save
the command, exit code, and error text; do not include account credentials.

| Exit | Meaning | First check |
|---|---|---|
| `0` | Successful operation or successful dry-run | Read the summary; a dry-run is not an install |
| `1` | Configuration/input error | Manifest syntax, checksum format, paths, and declared versions |
| `2` | Network/download error | Connectivity and HTTP error; do not delete a working mod merely to retry |
| `3` | Installation/runtime/filesystem error | Java/Fabric execution, permissions, and offline prerequisites |
| `4` | Verification error | Digest mismatch or expected installed/profile state missing; investigate rather than replacing the expected hash |

Invalid command-line syntax is handled by `clap` and can exit `2` before the
installer pipeline starts; read the accompanying usage error to distinguish it
from a download error.

Common cases:

- **Manifest parse failure:** fix the named file. Renaming/removing a local
  override opts back into the embedded catalog; do that only deliberately.
- **Minecraft directory missing:** launch vanilla once or pass the correct
  existing directory through `--mc-dir`.
- **Checksum mismatch:** inspect the URL/version and expected/actual digests.
  Do not remove the checksum or regenerate it merely to suppress the error.
- **Offline prerequisites missing:** use a normal connected install first.
  `--no-download` cannot populate an empty installation.
- **Game reports duplicate or incompatible mods:** the installer preserves
  unrelated/old filenames. Inspect the mod folder and compatibility against the
  server. This requires game-level investigation, not merely a successful hash.
- **Java/Fabric error:** distinguish the private installer runtime from the
  launcher's game runtime. Include `--verbose` output when asking for help.

## Automated verification

```bash
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

Tests must use temporary directories and injected network/Fabric behavior. They
must not require the live Minecraft server or modify your real game installation.
See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the final test commands, coverage,
verification record, and limitations.

The GitHub Actions workflow has Ubuntu and Windows jobs and packages a Windows
executable as an **Actions artifact**. This is not automatic GitHub Release
publication. Workflow definitions are not evidence that a remote run passed.

## Manual Windows release checklist

Allow roughly three minutes for checks **after** launcher setup and downloads;
first-time network downloads can take longer. Run this before distributing a
release, in a disposable installation with a backup of any copied launcher data.
Do not assume a unit-test smoke pass substitutes for these real checks.

1. Confirm the live server version matches the manifest. Open the official
   launcher and prepare a disposable vanilla installation for that version.
2. Close the launcher/game. Run the built executable with `--dry-run --mc-dir`
   pointing to the disposable directory. Verify expected versions and mod count,
   and confirm it did not change that directory.
3. Run the normal installer against the same directory. Confirm its successful
   summary, expected Fabric profile, mod jars, profile name/icon, and heap setting.
   If Java was absent, confirm the private installer runtime is provisioned.
4. Run again. Confirm matching mod files are skipped rather than downloaded.
5. Reopen the launcher, select the Currspice profile, and launch. Confirm the game
   reaches its menu with no dependency errors and can join the actual server.
6. Record OS, executable version/commit, manifest version, commands, pass/fail,
   and any error output. Do not mark the release gate complete if game launch or
   server joining was skipped.

The deterministic tests cover corrupted responses and repair paths. Do not
corrupt your daily-use installation to repeat those tests manually.

## Known boundaries and next steps

- Windows x64 is the supported end-user target; Ubuntu CI exercises portable
  code/tests, not a supported Linux desktop installer distribution.
- Existing unrelated mods are preserved. A mod upgrade with a new jar filename
  can leave its old version behind; check for duplicate versions before playing.
- There is no global rollback of an entire multi-step install. A later failure
  can leave earlier successful steps completed; inspect the error and rerun.
- No app/backend, login, remote server controls, backups, or release auto-updater
  are implemented yet.
- The next implementation phase is versioned manifest parsing and optional-mod
  selection, **after** release validation of the repaired installer.

See [ROADMAP.md](ROADMAP.md) for approved scope and
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for concrete, bounded follow-up tasks.
