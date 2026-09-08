//! The installer binary's CLI contract, exercised as a real subprocess:
//! help/version output, exit-code semantics (0 ok · 1 config · 2 network ·
//! 3 install-run · 4 verify), and offline flags. Hermetic: no network, no
//! real .minecraft, no Java on the PATH for these cases.
use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("currspice_mods").unwrap()
}

/// Names of the (non-recursive) entries in a directory, in directory order.
fn dir_names(p: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(p)
        .unwrap()
        .filter_map(std::io::Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Help & version — the "self-contained" P7 contract
// ---------------------------------------------------------------------------

#[test]
fn help_lists_every_documented_flag() {
    let assert = bin().args(["--help"]).assert().success();
    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    for flag in [
        "Usage:",
        "--mc-dir",
        "--config",
        "--dry-run",
        "--verbose",
        "--no-download",
    ] {
        assert!(stdout.contains(flag), "help must mention {flag}: {stdout}");
    }
}

#[test]
fn help_does_not_require_network() {
    // `--help` must short-circuit clap parsing before any manifest/network work.
    bin()
        .args(["--help"])
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .assert()
        .success();
}

#[test]
fn version_reports_crate_version() {
    let v = env!("CARGO_PKG_VERSION");
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(v));
}

#[test]
fn short_help_matches_long_help_flag_set() {
    let short_assert = bin().arg("-h").assert().success();
    let long_assert = bin().arg("--help").assert().success();
    let short_str = String::from_utf8_lossy(&short_assert.get_output().stdout).to_string();
    let long_str = String::from_utf8_lossy(&long_assert.get_output().stdout).to_string();
    let short = short_str.clone();
    let long = long_str.clone();
    for flag in [
        "--mc-dir",
        "--config",
        "--dry-run",
        "--verbose",
        "--no-download",
    ] {
        assert!(short.contains(flag), "-h missing {flag}");
        assert!(long.contains(flag), "--help missing {flag}");
    }
}

#[test]
fn unknown_flag_is_a_clap_usage_error_nonzero_exit() {
    // clap errors go to stderr and the process exits non-zero (2 by default).
    bin()
        .arg("--definitely-not-a-flag")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error").or(predicate::str::contains("Usage")));
}

// ---------------------------------------------------------------------------
// Config-error paths → exit code 1, human message, no side effects
// ---------------------------------------------------------------------------

#[test]
fn missing_mc_dir_is_config_error_exit_1() {
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .args([
            "--mc-dir",
            &tmp.path().join("nope").display().to_string(),
            "--config",
            "nonexistent.yaml",
        ])
        .current_dir(&tmp)
        .assert()
        .code(1);
}

#[test]
fn invalid_explicit_manifest_is_config_error_exit_1() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("bad.yaml");
    std::fs::write(&bad,
        "server_mods: [not-a-url]\nmc_version: 1.21.11\ninstaller_version: 0.18.4\nfabric: https://x/f.jar\n").unwrap();
    bin()
        .args(["--config", bad.to_str().unwrap(), "--dry-run"])
        .current_dir(&tmp)
        .assert()
        .code(1);
}

#[test]
fn dry_run_with_valid_manifest_and_missing_mc_dir_still_config_error() {
    // Even a dry run needs a resolvable mc-dir (P7: fail loud, early, clear).
    let tmp = tempfile::tempdir().unwrap();
    // Use the built-in manifest (no --config, no CWD override) but a bad dir.
    bin()
        .args([
            "--dry-run",
            "--mc-dir",
            tmp.path().join("missing").to_str().unwrap(),
        ])
        .current_dir(&tmp)
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
    let before = dir_names(&mc);
    bin()
        .args(["--dry-run", "--mc-dir", mc.to_str().unwrap()])
        .current_dir(&tmp)
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN"));
    let after = dir_names(&mc);
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
    let assert = bin()
        .args(["--no-download", "--mc-dir", mc.to_str().unwrap()])
        .current_dir(&tmp)
        .assert()
        .failure();
    let out = assert.get_output();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("Offline"), "must say offline: {stderr}");
    assert!(
        stderr.contains("no-download") || stderr.contains("dry-run"),
        "should name at least one escape: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Verbose adds support detail without changing the exit contract
// ---------------------------------------------------------------------------

#[test]
fn verbose_dry_run_emits_mod_and_fabric_url_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let mc = tmp.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    let mut cmd = bin();
    let mc_arg = tmp.path().join("mc").display().to_string();
    cmd.args(["--dry-run", "--mc-dir", mc_arg.as_str(), "--verbose"]);
    cmd.env_remove("RUST_BACKTRACE");
    cmd.current_dir(&tmp);
    let out = cmd.output().expect("binary should run");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "dry-run should exit 0; stderr={stderr}"
    );
    assert!(
        stderr.contains("Fabric URL"),
        "verbose must emit the Fabric URL: {stderr}"
    );
    assert!(
        stderr.contains("sha256") || stderr.contains("SHA-256"),
        "verbose should pin the hash: {stderr}"
    );
}

#[test]
fn verbose_error_shows_detail_not_the_non_verbose_hint() {
    // --no-download hits the offline error before any network. Verbose must
    // print the real reason and must NOT repeat the non-verbose
    // "re-run with --verbose" hint.
    let tmp = tempfile::tempdir().unwrap();
    let mc = tmp.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    let assert = bin()
        .args([
            "--no-download",
            "--mc-dir",
            mc.to_str().unwrap(),
            "--verbose",
        ])
        .current_dir(&tmp)
        .assert()
        .failure();
    let out = assert.get_output();
    let err_text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        err_text.contains("Offline"),
        "verbose should show the real error: {err_text}"
    );
    assert!(
        !err_text.contains("re-run with --verbose"),
        "verbose must not repeat the non-verbose hint: {err_text}"
    );
}

// ---------------------------------------------------------------------------
// The exit-code table is a public API — each class maps to exactly one code
// ---------------------------------------------------------------------------

#[test]
fn exit_config_is_1() {
    // Missing mc-dir, built-in manifest: config-class failure.
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .args(["--dry-run", "--mc-dir", "nonexistent-dir-name"])
        .current_dir(&tmp)
        .assert()
        .code(1);
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
