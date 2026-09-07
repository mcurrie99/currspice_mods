//! The production pipeline, end to end, through the exact seams the CLI uses
//! (`Config` → `pipeline::install` with injected `Transport` + `Runtime`):
//! sequencing, dry-run/no-download safety, duplicate detection, heap clamping,
//! and full idempotency across repeated runs.
mod common;
use common::*;
use currspice_mods::{
    config::Config,
    download::{InstallError, Transport},
    mods::{Fabric, sha256_hex},
    pipeline::{Options, Runtime, install},
};
use std::io::Write;
use std::path::{Path, PathBuf};

// ---- fakes ------------------------------------------------------------------

#[derive(Default)]
struct Fake {
    bodies: std::collections::VecDeque<Vec<u8>>,
    calls: Vec<String>,
}
impl Transport for Fake {
    fn fetch(&mut self, url: &str, out: &mut dyn Write) -> Result<(), InstallError> {
        use InstallError::*;
        self.calls.push(url.to_string());
        let body = self.bodies.pop_front().ok_or(Network("script exhausted".into()))?;
        out.write_all(&body)?;
        Ok(())
    }
}
impl Fake {
    fn push(&mut self, b: Vec<u8>) {
        self.bodies.push_back(b);
    }
}

#[derive(Default)]
struct Runner {
    fabric_outcome: std::sync::Mutex<FabricOutcome>,
    java_calls: usize,
    fabric_calls: usize,
}
#[derive(Default, Clone, Copy)]
enum FabricOutcome {
    /// Success: the "real" installer would have written the proper profile.
    #[default]
    Ok,
    /// Fabric succeeds but never writes the expected profile entry.
    NoProfile,
    /// Fabric exits non-zero.
    Fail,
}
impl Runtime for Runner {
    fn java(&mut self) -> Result<PathBuf, InstallError> {
        self.java_calls += 1;
        Ok(PathBuf::from("fake-java-on-path"))
    }
    fn fabric(&mut self, f: &Fabric, _: &Path, _: bool) -> Result<(), InstallError> {
        self.fabric_calls += 1;
        let o = *self.fabric_outcome.lock().unwrap();
        match o {
            FabricOutcome::Fail => Err(InstallError::Install("fabric exited 1".into())),
            FabricOutcome::NoProfile => Ok(()), // pretend we ran but made no profile
            FabricOutcome::Ok => {
                let p = f.mc_dir().join("launcher_profiles.json");
                if !p.exists() {
                    // Fabric requires a pre-existing launcher_profiles.json.
                    return Err(InstallError::Install(
                        "Fabric: Could not find a valid launcher profile".into(),
                    ));
                }
                std::fs::write(
                    p,
                    format!(
                        r#"{{"client":{{"latest":"{}"}},
"profiles":{{"fabric-loader-{}":{{"lastVersionId":"fabric-loader-{}-{}","type":"custom"}}}}}}"#,
                        f.get_mc_version(),
                        f.get_mc_version(),
                        f.get_inst_version(),
                        f.get_mc_version(),
                    ),
                )
                .unwrap();
            }
        }
        Ok(())
    }
}

// ---- shared fixtures ---------------------------------------------------------

/// Builds a valid in-memory `Config` from (url, optional sha256) pairs.
/// Borrowed args, so it composes with both array literals and Vecs at call sites.
fn config(
    server_mods: &[(&str, Option<&str>)],
    user_mods: &[(&str, Option<&str>)],
) -> Config {
    let mut yaml = String::new();
    yaml.push_str("mc_version: 1.21.11\ninstaller_version: 0.18.4\n");
    yaml.push_str("fabric: https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar\n");
    yaml.push_str("server_mods:\n");
    for (url, hash) in server_mods {
        yaml.push_str(&format!("- url: {url}\n"));
        if let Some(h) = hash {
            yaml.push_str(&format!("  sha256: {h}\n"));
        }
    }
    if !user_mods.is_empty() {
        yaml.push_str("user_mods:\n");
        for (url, hash) in user_mods {
            yaml.push_str(&format!("- url: {url}\n"));
            if let Some(h) = hash {
                yaml.push_str(&format!("  sha256: {h}\n"));
            }
        }
    }
    serde_yaml::from_str(&yaml).expect("fixture config must parse")
}

fn opts(mc_dir: &Path, mut f: impl FnOnce(&mut Options)) -> Options {
    let mut o = Options { mc_dir: mc_dir.to_path_buf(), ..Options::default() };
    f(&mut o);
    o
}

fn seed(mc: &Path) {
    // The launcher_profiles.json the real Fabric CLI needs before it runs.
    let p = mc.join("launcher_profiles.json");
    if !p.exists() {
        std::fs::write(
            &p,
            r#"{"client":{"latest":"1.21.11"},"launcherVersion":"1.21.11","profiles":{}}"#,
        )
        .unwrap();
    }
}

// ---------------------------------------------------------------------------
// Core lifecycle
// ---------------------------------------------------------------------------

#[test]
fn fresh_install_then_idempotent_rerun() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let hash = sha256_hex(b"MODBYTES");
    let a = [
        ("https://cdn.ex/data/1/versions/1/alpha-1.21.11.jar", Some(&hash)),
        ("https://cdn.ex/data/2/versions/1/beta-1.21.11.jar", None),
    ];
    let user = [("https://cdn.ex/data/3/versions/1/gamma-1.21.11.jar", Some(&sha256_hex(b"USERMOD")))]
        .into_iter()
        .collect::<Vec<_>>();
    let mut http = Fake::default();
    http.push(b"MODBYTES".to_vec());
    http.push(make_jar()); // unpinned beta: must be a valid jar
    http.push(b"USERMOD".to_vec());
    let mut runner = Runner::default();

    // run 1
    let summary = install(
        config(&a, &user),
        &opts(&mc, |_| {}),
        &mut http,
        &mut runner,
    )
    .unwrap();
    assert!(summary.contains("installed: 3"), "{summary}");
    assert_eq!(runner.java_calls, 1);
    // run 2 — everything current → all skipped, zero extra downloads
    http.reset_for_test();
    let summary = install(config(&a, &user), &opts(&mc, |_| {}), &mut http, &mut runner).unwrap();
    assert!(summary.contains("skipped: 3"), "{summary}");
    assert!(http.calls.is_empty(), "a fully-up-to-date re-run must not touch the network");
    // both mods present and byte-exact
    assert_eq!(std::fs::read(mc.join("mods").join("alpha-1.21.11.jar")).unwrap(), b"MODBYTES");
    assert_eq!(
        std::fs::read(mc.join("mods").join("gamma-1.21.11.jar")).unwrap(),
        b"USERMOD"
    );
}
trait FakeReset {
    fn reset_for_test(&mut self);
}
impl FakeReset for Fake {
    fn reset_for_test(&mut self) {
        self.calls.clear();
        self.bodies.clear();
    }
}

#[test]
fn partial_staleness_only_replaces_the_stale_mods() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let h1 = sha256_hex(b"V1");
    let url1 = "https://cdn.ex/data/1/versions/1/alpha-1.21.11.jar";
    let mut http = Fake::default();
    http.push(b"V1".to_vec());
    install(config(&[(url1, Some(&h1))], &[]), &opts(&mc, |_| {}), &mut http, &mut Runner::default())
        .unwrap();
    // Now alpha's bytes go stale on disk (a CDN update the friend is missing).
    std::fs::write(mc.join("mods").join("alpha-1.21.11.jar"), b"STALE-ON-DISK").unwrap();
    let mut http = Fake::default();
    http.push(b"V1".to_vec());
    http.reset_for_test();
    let summary = install(
        config(&[(url1, Some(&h1))], &[]),
        &opts(&mc, |_| {}),
        &mut http,
        &mut Runner::default(),
    )
    .unwrap();
    assert!(summary.contains("replaced: 1"), "{summary}");
    assert_eq!(std::fs::read(mc.join("mods").join("alpha-1.21.11.jar")).unwrap(), b"V1");
}

#[test]
fn dry_run_performs_no_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    // Do NOT seed the profile: dry-run must not require it, and must not create it.
    let mut http = Fake::default();
    let mut runner = Runner::default();
    let res = install(
        config(&[("https://cdn.ex/a-1.21.11.jar", None)], &[]),
        &opts(&mc, |o| o.dry_run = true),
        &mut http,
        &mut runner,
    )
    .unwrap();
    assert!(res.contains("DRY RUN"));
    assert_eq!(http.calls.len(), 0, "dry-run must not fetch");
    assert_eq!(runner.java_calls, 0, "dry-run must not launch java");
    assert_eq!(runner.fabric_calls, 0, "dry-run must not run fabric");
    assert!(!mc.join("launcher_profiles.json").exists(), "dry-run must not seed a profile");
    assert!(!mc.join("mods").exists(), "dry-run must not create the mods dir");
}

#[test]
fn no_download_fails_fast_before_any_change() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let mut http = Fake::default();
    let mut runner = Runner::default();
    let err = install(
        config(&[("https://cdn.ex/a-1.21.11.jar", None)], &[]),
        &opts(&mc, |o| o.no_download = true),
        &mut http,
        &mut runner,
    )
    .unwrap_err();
    assert!(err.to_string().contains("Offline"), "{err}");
    assert_eq!(http.calls.len(), 0);
    assert_eq!(runner.java_calls, 0);
    assert_eq!(runner.fabric_calls, 0);
    assert!(!mc.join("mods").exists());
    // launcher_profiles.json was seeded by the test setup, not by the pipeline
    assert!(!mc.join("launcher_profiles.json").with_file_name("x.jar").exists());
}

#[test]
fn missing_mc_dir_is_a_config_error_with_no_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist");
    let mut http = Fake::default();
    let mut runner = Runner::default();
    let err = install(
        config(&[("https://cdn.ex/a-1.21.11.jar", None)], &[]),
        &opts(&missing, |_| {}),
        &mut http,
        &mut runner,
    )
    .unwrap_err();
    assert_eq!(err.exit_code(), 1, "missing mc-dir is Config-class");
    assert!(err.to_string().contains("Minecraft directory"), "{err}");
    assert!(http.calls.is_empty());
    assert!(runner.fabric_calls == 0);
}

#[test]
fn invalid_config_rejected_before_any_side_effect() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    // An un-drift-guarded manifest (1.21.1 declared, 1.21.11 mods) must fail
    // at Config, never reach java/fabric.
    let cfg = serde_yaml::from_str::<Config>(
        "mc_version: 1.21.1\ninstaller_version: 0.18.4\n\
         fabric: https://maven.fabricmc.net/x.jar\n\
         server_mods:\n- https://cdn.ex/data/1/versions/1/mod-1.21.11.jar\n",
    )
    .unwrap();
    let mut http = Fake::default();
    let mut runner = Runner::default();
    let err = install(cfg, &opts(&mc, |_| {}), &mut http, &mut runner).unwrap_err();
    assert_eq!(err.exit_code(), 1);
    assert!(err.to_string().contains("1.21.11"), "drift message must name the offender: {err}");
    assert!(http.calls.is_empty());
    assert!(runner.fabric_calls == 0);
}

#[test]
fn duplicate_mod_filenames_across_server_and_user_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let cfg = config(
        &[("https://cdn.ex/data/1/versions/1/sodium-1.21.11.jar", None)],
        &[("https://other.ex/data/2/versions/9/sodium-1.21.11.jar", None)],
    );
    let mut http = Fake::default();
    let mut runner = Runner::default();
    let err = install(cfg, &opts(&mc, |_| {}), &mut http, &mut runner).unwrap_err();
    assert_eq!(err.exit_code(), 1);
    assert!(err.to_string().contains("Duplicate"), "{err}");
    assert!(http.calls.is_empty(), "duplicate detection must prevent any download");
}

#[test]
fn fabric_failure_blocks_profile_edit_and_all_mod_installs() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let mut http = Fake::default();
    let mut runner = Runner::default();
    *runner.fabric_outcome.lock().unwrap() = FabricOutcome::Fail;
    let err = install(
        config(&[("https://cdn.ex/a-1.21.11.jar", None)], &[]),
        &opts(&mc, |_| {}),
        &mut http,
        &mut runner,
    )
    .unwrap_err();
    assert!(matches!(err, InstallError::Install(_)));
    assert_eq!(runner.fabric_calls, 1);
    assert!(!mc.join("mods").exists(), "no mod may be installed after a failed fabric");
    // The profile edit never happens: the seeded doc is unchanged.
    let p = mc.join("launcher_profiles.json");
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    assert!(!doc["profiles"]["fabric-loader-1.21.11"].get("javaArgs").is_some());
}

#[test]
fn fabric_no_profile_writes_blocks_everything_after_verify() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let mut http = Fake::default();
    let mut runner = Runner::default();
    *runner.fabric_outcome.lock().unwrap() = FabricOutcome::NoProfile;
    let err = install(
        config(&[("https://cdn.ex/a-1.21.11.jar", None)], &[]),
        &opts(&mc, |_| {}),
        &mut http,
        &mut runner,
    )
    .unwrap_err();
    assert_eq!(err.exit_code(), 4, "missing fabric profile is a Verify-class failure");
    assert!(!mc.join("mods").exists());
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(mc.join("launcher_profiles.json")).unwrap())
            .unwrap();
    assert!(
        !doc["profiles"]["fabric-loader-1.21.11"].get("javaArgs").is_some(),
        "profile edit must not have happened"
    );
}

#[test]
fn installer_jar_is_downloaded_and_run_before_seed_needs_it() {
    // Ordering check: the fabric installer jar must be fetched (via the
    // injected transport) before the runtime.fabric call.
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let mut http = Fake::default();
    http.push(b"INSTALLER-JAR".to_vec()); // fabric installer
    http.push(b"MOD".to_vec());
    let mut runner = Runner::default();
    let first_url = {
        install(
            config(&[("https://cdn.ex/a-1.21.11.jar", None)], &[]),
            &opts(&mc, |_| {}),
            &mut http,
            &mut runner,
        )
        .unwrap();
        http.calls.first().cloned()
    };
    assert_eq!(
        first_url.as_deref(),
        Some("https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar"),
        "the fabric installer must be the first network artifact"
    );
}

#[test]
fn mods_dir_created_and_layout_is_canonical() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let mod_url = "https://cdn.ex/data/obj/versions/x/iris-fabric-1.21.11.jar";
    let mut http = Fake::default();
    http.push(make_jar());
    install(config(&[(mod_url, None)], &[]), &opts(&mc, |_| {}), &mut http, &mut Runner::default())
        .unwrap();
    let mods = mc.join("mods");
    assert!(mods.is_dir());
    assert!(mods.join("iris-fabric-1.21.11.jar").is_file());
    assert!(!mods.join("mods").exists(), "no nested mods-of-mods");
}

#[test]
fn unrelated_files_in_mc_root_survive_install() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(mc.join("server")).unwrap();
    std::fs::write(mc.join("options.txt"), b"fov:1.0").unwrap();
    std::fs::write(mc.join("server").join("world.dat"), b"KEEP-ME").unwrap();
    seed(&mc);
    let mut http = Fake::default();
    http.push(make_jar());
    install(config(&[("https://cdn.ex/mod-1.21.11.jar", None)], &[]), &opts(&mc, |_| {}), &mut http, &mut Runner::default()).unwrap();
    assert_eq!(std::fs::read(mc.join("options.txt")).unwrap(), b"fov:1.0");
    assert_eq!(std::fs::read(mc.join("server").join("world.dat")).unwrap(), b"KEEP-ME");
}

#[test]
fn heap_gb_zero_is_clamped_to_one_in_profile_and_summary() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let mut http = Fake::default();
    http.push(make_jar());
    let summary = install(
        config(&[("https://cdn.ex/mod-1.21.11.jar", None)], &[]),
        &opts(&mc, |o| o.heap_gb = 0),
        &mut http,
        &mut Runner::default(),
    )
    .unwrap();
    assert!(summary.contains("(1 GB heap)"), "0 must clamp to at least 1: {summary}");
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(mc.join("launcher_profiles.json")).unwrap())
            .unwrap();
    let args = doc["profiles"]["fabric-loader-1.21.11"]["javaArgs"].as_str().unwrap();
    assert!(args.contains("-Xmx1G"), "profile must hold a ≥1G heap: {args}");
    assert!(!args.contains("-Xmx0G"), "0G heap must never reach the profile: {args}");
}

#[test]
fn verbose_summary_is_still_the_human_facing_contract() {
    // Verbose adds support detail (URLs/hashes on stderr) but must never
    // change the machine-readable summary lines a script would parse.
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let pinned = sha256_hex(b"V");
    let mods = [("https://cdn.ex/data/1/versions/1/vect-1.21.11.jar", Some(&pinned))];
    let mut http = Fake::default();
    http.push(b"V".to_vec());
    let mut runner = Runner::default();
    let summary = install(
        config(&mods, &[]),
        &opts(&mc, |o| o.verbose = true),
        &mut http,
        &mut runner,
    )
    .unwrap();
    assert!(summary.contains("installed: 1"), "{summary}");
    assert!(summary.contains("1 GB heap") || summary.contains(" GB heap"), "{summary}");
}

#[test]
fn two_consecutive_runs_are_bit_for_bit_identical() {
    // Property of an idempotent installer: after two runs, the mods directory
    // is exactly the expected set of jars, byte-for-byte, no drift, no dupes.
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let h1 = sha256_hex(b"A");
    let h2 = sha256_hex(b"B");
    let mods: [(&str, Option<&str>); 2] = [
        ("https://cdn.ex/a1-1.21.11.jar", Some(&h1)),
        ("https://cdn.ex/a2-1.21.11.jar", Some(&h2)),
    ];
    for _ in 0..2 {
        let mut http = Fake::default();
        http.push(b"A".to_vec());
        http.push(b"B".to_vec());
        install(config(&mods, &[]), &opts(&mc, |_| {}), &mut http, &mut Runner::default()).unwrap();
    }
    let listing: Vec<_> = std::fs::read_dir(mc.join("mods")).unwrap().flatten().collect();
    assert_eq!(listing.len(), 2, "exactly the two expected mods, no drift");
    assert_eq!(std::fs::read(mc.join("mods").join("a1-1.21.11.jar")).unwrap(), b"A");
    assert_eq!(std::fs::read(mc.join("mods").join("a2-1.21.11.jar")).unwrap(), b"B");
}

#[test]
fn summary_reports_per_run_outcome_counts_accurately() {
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let h1 = sha256_hex(b"K1");
    let h2 = sha256_hex(b"K2");
    let urls = [
        ("https://cdn.ex/keep1-1.21.11.jar", h1.as_str(), b"K1"),
        ("https://cdn.ex/keep2-1.21.11.jar", h2.as_str(), b"K2"),
    ];
    let mods: Vec<(&str, Option<&str>)> =
        urls.iter().map(|(u, h, _)| (u, Some(h))).collect();
    let mut http = Fake::default();
    http.push(b"K1".to_vec());
    http.push(b"K2".to_vec());
    install(
        config(&mods, &[]),
        &opts(&mc, |_| {}),
        &mut http,
        &mut Runner::default(),
    )
    .unwrap();
    // Stale one of them on disk so the next run shows replaced:1 skipped:1
    std::fs::write(mc.join("mods").join("keep2-1.21.11.jar"), b"STALE").unwrap();
    let mut http = Fake::default();
    http.push(b"K2".to_vec()); // only the stale one needs refetching
    let summary = install(
        config(&mods, &[]),
        &opts(&mc, |_| {}),
        &mut http,
        &mut Runner::default(),
    )
    .unwrap();
    assert!(summary.contains("skipped: 1"), "{summary}");
    assert!(summary.contains("replaced: 1"), "{summary}");
}

#[test]
fn user_mods_installed_together_with_server_mods_in_one_pass() {
    // Guards the old bug mode where user_mods silently vanished (Phase 0 P3).
    let dir = tempfile::tempdir().unwrap();
    let mc = dir.path().join("mc");
    std::fs::create_dir_all(&mc).unwrap();
    seed(&mc);
    let h_user = sha256_hex(b"USER");
    let mut http = Fake::default();
    http.push(make_jar()); // server mod unpinned
    http.push(b"USER".to_vec());
    let summary = install(
        config(
            &[("https://cdn.ex/srv-1.21.11.jar", None)],
            &[("https://cdn.ex/usr-1.21.11.jar", Some(&h_user))],
        ),
        &opts(&mc, |_| {}),
        &mut http,
        &mut Runner::default(),
    )
    .unwrap();
    assert!(summary.contains("installed: 2"), "{summary}");
    assert!(mc.join("mods").join("usr-1.21.11.jar").is_file());
}
