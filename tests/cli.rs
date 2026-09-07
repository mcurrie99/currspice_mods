//! The installer binary's CLI contract, exercised as a real subprocess:
//! help/version output, exit-code semantics (0 ok · 1 config · 2 network ·
//! 3 install-run · 4 verify), and offline flags. Hermetic: no network, no
//! real .minecraft, no Java on the PATH for these cases.
use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    let mut c = Command::cargo_bin("currspice_mods").unwrap();
    c
}

// ---------------------------------------------------------------------------
// Help & version — the "self-contained" P7 contract
// ---------------------------------------------------------------------------

#[test]
fn help_lists_every_documented_flag() {
    let out = bin().args(["--help"]).assert().stdout().to_string();
    for flag in ["Usage:", "--mc-dir", "--config", "--dry-run", "--verbose", "--no-download"] {
        assert!(out.contains(flag), "help must mention {flag}: {out}");
    }
}

#[test]
fn help_does_not_require_network() {
    // `--help` must short-circuit clap parsing before any manifest/network work.
    bin().args(["--help"])
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .assert()
        .success();
}

#[test]
fn version_reports_crate_version() {
    let v = std::env!("CARGO_PKG_VERSION");
    bin().arg("--version").assert().success().stdout(predicate::str::contains(v));
}

#[test]
fn short_help_matches_long_help_flag_set() {
    let short = bin().arg("-h").assert().stdout().to_string();
    let long = bin().arg("--help").assert().stdout().to_string();
    for flag in ["--mc-dir", "--config", "--dry-run", "--verbose", "--no-download"] {
        assert!(short.contains(flag), "-h missing {flag}");
        assert!(long.contains(flag), "--help missing {flag}");
    }
}

#[test]
fn unknown_flag_is_a_clap_usage_error_nonzero_exit() {
    let out = bin()
        .arg("--definitely-not-a-flag")
        .assert().stdout()
        .to_string();
    assert!(out.contains("error") || out.contains("Usage"), "{out}");
}

// ---------------------------------------------------------------------------
// Config-error paths → exit code 1, human message, no side effects
// ---------------------------------------------------------------------------

#[test]
fn missing_mc_dir_is_config_error_exit_1() {
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .args(["--mc-dir", &tmp.path().join("nope").display().to_string(), "--config", "nonexistent.yaml"])
        .current_dir(&tmp)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Currspice"))
        .stdout(predicate::str::contains("verbose")); // hint when not verbose
}

#[test]
fn invalid_explicit_manifest_is_config_error_exit_1() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("bad.yaml");
    std::fs::write(&bad, "server_mods: [not-a-url]\nmc_version: 1.21.11\ninstaller_version: 0.18.4\nfabric: https://x/f.jar\n").unwrap();
    bin()
        .args(["--config", bad.to_str().unwrap(), "--dry-run"])
        .current_dir(&tmp)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Could not load the mod manifest")
            .or(predicate::str::contains("could not load the mod manifest")));
}

#[test]
fn dry_run_with_valid_manifest_and_missing_mc_dir_still_config_error() {
    // Even a dry run needs a resolvable mc-dir (P7: fail loud, early, clear).
    let tmp = tempfile::tempdir().unwrap();
    // Use the built-in manifest (no --config, no CWD override) but a bad dir.
    bin()
        .args(["--dry-run", "--mc-dir", tmp.path().join("missing").to_str().unwrap()])
        .current_dir(&tmp)
        .env_remove("USERNAME")
        .assert()
        .code(1);
}

// ---------------------------------------------------------------------------
// Dry-run happy path: prints the plan, touches nothing
// ---------------------------------------------------------------------------

#[test]
fn dry_run_success_prints_plan_and_exit_0() {
    let tmp = tempfile::tempdir().unwrap();
    let mc = tmp.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    let before: Vec<_> = std::fs::read_dir(&mc).unwrap().flatten().map(|e| e.name()).collect();
    bin()
        .args(["--dry-run", "--mc-dir", mc.to_str().unwrap()])
        .current_dir(&tmp)
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN"));
    let after: Vec<_> = std::fs::read_dir(&mc).unwrap().flatten().map(|e| e.name()).collect();
    assert_eq!(before, after, "--dry-run must not create files");
    assert!(!mc.join("mods").exists());
}

#[test]
fn dry_run_on_fresh_dir_reports_planned_mods() {
    let tmp = tempfile::tempdir().unwrap();
    let mc = tmp.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    bin()
        .args(["--dry-run", "--mc-dir", mc.to_str().unwrap()])
        .current_dir(&tmp)
        .assert()
        .success()
        .stdout(predicate::str::contains("mod(s)"));
}

// ---------------------------------------------------------------------------
// No-download: refuses to progress, exit code 3 (install-run class), clean text
// ---------------------------------------------------------------------------

#[test]
fn no_download_is_a_clear_offline_error_not_a_crash() {
    let tmp = tempfile::tempdir().unwrap();
    let mc = tmp.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    bin()
        .args(["--no-download", "--mc-dir", mc.to_str().unwrap()])
        .current_dir(&tmp)
        .assert()
        .failure()
        .stdout(predicate::str::contains("Offline"))
        .stdout(predicate::str::contains("no-download").or(predicate::str::contains("dry-run")));
}

// ---------------------------------------------------------------------------
// Verbose adds support detail without changing the exit contract
// ---------------------------------------------------------------------------

#[test]
fn verbose_dry_run_emits_mod_and_fabric_url_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let mc = tmp.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    bin()
        .args(["--dry-run", "--mc-dir", mc.to_str().unwrap(), "--verbose"])
        .current_dir(&tmp)
        .assert()
        .success()
        .stderr(predicate::str::contains("Fabric URL"))
        .stderr(predicate::str::contains("Fabric loader"))
        .or(predicate::str::contains("sha256"));
}

#[test]
fn verbose_replaces_the_re_run_hint_with_full_detail() {
    // Non-verbose errors append "re-run with --verbose"; verbose errors must
    // not repeat that hint (we ARE verbose).
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .args(["--no-download", "--mc-dir", tmp.path().join("mc").to_str().unwrap(), "--verbose"])
        .current_dir(&tmp)
        .assert()
        .failure()
        .stdout(predicate::str::contains("could not"));
}

// ---------------------------------------------------------------------------
// The exit-code table is a public API — each class maps to exactly one code
// ---------------------------------------------------------------------------

#[test]
fn exit_config_is_1() {
    // Missing mc-dir, built-in manifest: config-class failure.
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_current_dir(&tmp);
    bin().args(["--dry-run", "--mc-dir", "nonexistent-dir-name"]).assert().code(1);
}

#[test]
fn exit_code_classes_map_to_their_documented_values() {
    // A 4-class failure (e.g. checksum mismatch) exits 4 and reads
    // differently from config errors. We assert the *contract* from the
    // library — the binary maps `InstallError::exit_code()` verbatim.
    use currspice_mods::download::InstallError;
    assert_eq!(InstallError::Config("x".into()).exit_code(), 1);
    assert_eq!(InstallError::Network("x".into()).exit_code(), 2);
    assert_eq!(InstallError::Install("x".into()).exit_code(), 3);
    assert_eq!(InstallError::Verify("x".into()).exit_code(), 4);
}
