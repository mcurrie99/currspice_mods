# Coding-agent guide

This file is the starting point for any coding agent, including a local Qwen agent.
It describes project constraints and a repeatable workflow, not permission to
implement every item in the roadmap. Work only on the user's requested scope.

## Read before editing

1. `README.md`: current supported behavior, commands, limitations, and testing.
2. `ROADMAP.md`: approved future architecture and phase acceptance criteria.
3. `docs/DEVELOPMENT.md`: module map, invariants, regression tests, and handoff.
4. The implementation and tests for the behavior being changed.
5. `git status --short --branch` and `git diff`: identify existing work first.

If these sources disagree, inspect code and run a small test. Record the
contradiction; do not silently choose the most convenient version. A roadmap
checkbox, comment, previous agent answer, or old binary is not proof that the
current source works.

## Non-negotiable project decisions

- The current deliverable is a Windows x64 **console installer** for the Currspice
  Minecraft SMP. Do not replace it with a GUI.
- The approved future GUI is **Currspice Home**, a Tauri app with an axum backend.
  These are later roadmap phases, not already implemented features.
- The future backend requires username/password login, argon2 password hashing,
  owner/admin/friend roles, owner-provisioned invitations, and audited operations.
  No public self-signup. Backup/restore and server controls are also later phases.
- The standalone installer must not require the future live backend or login.
- `docs/mods.yaml` is the canonical published mod manifest. The root `mods.yaml`
  is a user's local override and must stay out of Git. Never edit a real user's
  Minecraft installation or overwrite a local override as a test fixture.
- Mod catalog changes belong in manifest data, not hardcoded Rust lists.
- Use `hermes-*` branches for AI-created work. Continue `hermes-roadmap` when
  working there; do not switch branches with dirty work without authorization.
- Do not discard existing changes, run destructive Git cleanup, publish releases,
  push, deploy, or operate the Minecraft server unless explicitly requested.
- Never put passwords, session tokens, private endpoints, or credentials in docs,
  fixtures, Git, or test logs.

## Small, verifiable implementation steps

1. Name the exact behavior and relevant roadmap item.
2. Reproduce the bug or add a focused failing test. Run it and inspect the failure.
   If compilation already fails, preserve that output and restore the smallest
   necessary API consistency before testing behavior. Do not equate a compiler
   error with a behavioral assertion failure.
3. Implement the smallest coherent fix. Update all callers when changing a type.
4. Run the targeted test, then the full suite. Do not change assertions merely to
   match a broken implementation.
5. Review the diff for unrelated edits, data loss, unsafe paths, unchecked HTTP
   errors, and accidental network access in offline/dry-run paths.
6. Update README/development docs when behavior or commands change. Update roadmap
   status only when its acceptance criteria were actually exercised.
7. Report what changed, exact commands and outcomes, what was not tested, and the
   next bounded task. Do not claim a real install, successful CI run, or published
   release based only on a unit test or a workflow file.

Prefer one vertical behavior slice at a time. Avoid introducing a new framework,
workspace layout, dependency, backend, or UI merely to repair an installer bug.

## Windows development commands

Run commands from the repository root. On the maintainer's Hermes desktop host,
`terminal` uses **Git Bash, not PowerShell**, and Cargo may not be on PATH:

```bash
export PATH="/c/Users/matth/.cargo/bin:$PATH"
cargo --version
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

Alternatively use `"C:/Users/matth/.cargo/bin/cargo.exe"` for each Cargo command.
That absolute path is a local-machine convenience, not a path to embed in Rust
source or CI. Other machines can use their normal Cargo installation.

The Windows build needs the Rust MSVC toolchain, MSVC build tools and Windows SDK
(resource compilation embeds `icon.ico`). Native Windows programs should receive
paths such as `E:/OneDrive/Outside/Rust/currspice_mods`, not `/e/...`; the latter
works for Bash's `cd` but may not be translated for native tools.

Run Python with `python`, not `python3`, on this host. Use the commands in
`README.md` for generator checks and deterministic smoke tests.

## Verification boundaries

- Automated tests must use temporary directories and deterministic fixtures.
- A fake Fabric runner is evidence about installer orchestration, not proof that
  Mojang/Fabric downloads or the real game work today.
- Do not invoke manifest refresh merely to validate syntax: refresh can download
  every mod and change recorded hashes. Use the documented offline check first.
- Do not regenerate expected checksums to make a mismatch disappear. Investigate
  the source/version change and preserve the integrity failure until understood.
- A checksum verifies bytes against a trusted manifest; it is not a signature and
  does not authenticate a compromised manifest or publisher.
- Changes must preserve existing launcher profiles and unrelated mods. Do not
  delete a whole mods folder, JRE cache, or Minecraft directory as a repair shortcut.
- Release readiness includes the manual Windows checklist and actual CI results,
  not just successful local compilation.
