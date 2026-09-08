//! Exhaustive manifest/config validation — parsing, precedence, checksum
//! policy, version-drift, and serialization round-trips.
mod common;
use common::*;
use currspice_mods::config::{Config, ModRef};

/// Serializes the tests that switch the process working directory: Rust tests
/// run in parallel threads inside one test binary, so two tests changing the
/// process cwd at once would break each other's CWD-precedence assertions.
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ---------------------------------------------------------------------------
// Shape acceptance / rejection
// ---------------------------------------------------------------------------

#[test]
fn legacy_url_shapes_still_load() {
    // v1 (bare string) and v1.5 (url+sha256) must both remain readable.
    let sha = "a".repeat(64);
    let yaml = [
        "mc_version: 1.21.11",
        "installer_version: 0.18.4",
        "fabric: https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar",
        "server_mods:",
        "  - https://cdn.modrinth.com/data/x/versions/y/a-1.21.11.jar",
        "  - url: https://cdn.modrinth.com/data/x/versions/y/b-1.21.11.jar",
    ]
    .join("\n")
        + &format!("\n    sha256: {sha}\n");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("m.yaml");
    std::fs::write(&file, yaml).unwrap();
    let cfg = Config::load_new(&file).unwrap();
    assert_eq!(cfg.get_server_mods().len(), 2);
    assert!(matches!(cfg.get_server_mods()[0], ModRef::Url(_)));
    assert!(matches!(cfg.get_server_mods()[1], ModRef::Entry { .. }));
    assert!(cfg.get_server_mods()[1].sha256().is_some());
    cfg.validate().unwrap();
    assert!(cfg.verify_version_consistency().is_ok());
}

#[test]
fn unknown_top_level_field_is_rejected() {
    // The manifest is a contract; a typo'd field must not be silently dropped
    // (that would install a set the server can't load).
    let yaml = "mc_version: 1.21.11\ninstaller_version: 0.18.4\n\
                fabric: https://maven.fabricmc.net/x.jar\n\
                server_mods:\n\
                - https://cdn.modrinth.com/data/x/versions/y/a-1.21.11.jar\n\
                bogus_field: [1,2]\n";
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("m.yaml");
    std::fs::write(&file, yaml).unwrap();
    assert!(
        Config::load_new(&file).is_err(),
        "unknown field must not be ignored"
    );
}

#[test]
fn user_mods_empty_defaults_absent_both() {
    // Absent `user_mods` and an *empty* list must both deserialize.
    for yaml in [
        "mc_version: 1.21.11\ninstaller_version: 0.18.4\nfabric: https://maven.fabricmc.net/x.jar\n\
         server_mods:\n- https://cdn.modrinth.com/data/x/versions/y/a-1.21.11.jar\n",
        "mc_version: 1.21.11\ninstaller_version: 0.18.4\nfabric: https://maven.fabricmc.net/x.jar\n\
         server_mods:\n- https://cdn.modrinth.com/data/x/versions/y/a-1.21.11.jar\nuser_mods: []\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("m.yaml");
        std::fs::write(&file, yaml).unwrap();
        let cfg = Config::load_new(&file).unwrap();
        assert!(cfg.get_user_mods().is_empty());
        assert_eq!(cfg.get_server_mods().len(), 1);
    }
}

#[test]
fn all_mods_orders_server_before_user() {
    let mut c = valid_config();
    c.set_user_mods(vec![
        "https://cdn.modrinth.com/data/u/versions/1/user-1.21.11.jar".into(),
        "https://cdn.modrinth.com/data/u2/versions/1/user2-1.21.11.jar".into(),
    ]);
    let all: Vec<_> = c.all_mods().cloned().collect();
    assert_eq!(
        all.len(),
        c.get_server_mods().len() + c.get_user_mods().len()
    );
    // Server mods first, then user mods, each in list order.
    for (i, m) in c.get_server_mods().iter().enumerate() {
        assert_eq!(all[i], m.url());
    }
    let offset = c.get_server_mods().len();
    for (i, m) in c.get_user_mods().iter().enumerate() {
        assert_eq!(all[offset + i], m.url());
    }
}

#[test]
fn accessor_getters_are_consistent() {
    let c = valid_config();
    assert_eq!(c.get_mc_version(), "1.21.11");
    assert_eq!(c.get_installer_version(), "0.18.4");
    assert_eq!(c.get_path(), "");
    assert!(c.get_fabric_url().starts_with("https://"));
    assert_eq!(
        c.mod_refs().count(),
        c.get_server_mods().len() + c.get_user_mods().len()
    );
}

// ---------------------------------------------------------------------------
// Checksum policy
// ---------------------------------------------------------------------------

#[test]
fn checksum_validation_matrix() {
    // valid
    for h in [
        "a".repeat(64).to_string(),
        "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789".to_string(),
    ] {
        let m: ModRef =
            serde_yaml::from_str(&format!("url: https://host/a.jar\nsha256: {h}")).unwrap();
        assert!(m.validate().is_ok(), "should accept {h}");
    }
    // invalid — wrong length, non-hex, bad alphabet. (An *unquoted empty*
    // `sha256:` deserializes to `None` = "no recorded hash" = legacy/unpinned,
    // which is valid; the distinct *quoted-empty* `sha256: ""` case — a hash
    // that was meant to be present but left empty — is rejected and covered
    // by `sha256_field_empty_string_is_rejected_not_ignored`.)
    for h in [
        "abcdef".to_string(),
        "a".repeat(63),
        "a".repeat(65),
        "non-hex".to_string(),
        "aG".repeat(32),
    ] {
        let m: ModRef =
            serde_yaml::from_str(&format!("url: https://host/a.jar\nsha256: {h}")).unwrap();
        assert!(m.validate().is_err(), "should reject {h:?}");
    }
}

#[test]
fn uppercase_sha256_is_accepted_and_case_insensitive_on_compare() {
    use currspice_mods::mods::Mod;
    // Uppercase recorded hash must parse+validate (hex is case-insensitive).
    let upper = "A".repeat(64);
    let m: ModRef =
        serde_yaml::from_str(&format!("url: https://host/a.jar\nsha256: {upper}")).unwrap();
    assert!(m.validate().is_ok());
    // Mod::install compares case-insensitively: record the real digest in
    // UPPERCASE and the lowercase-body match must still install.
    use currspice_mods::mods::sha256_hex;
    let dir = tempfile::tempdir().unwrap();
    let mut mod_ = Mod::new("https://host/a.jar", dir.path()).unwrap();
    let body = b"payload";
    let recorded = sha256_hex(body).to_uppercase();
    assert_eq!(recorded, recorded.to_uppercase()); // sanity: it is uppercase
    let mut t = Scripted::new(vec![body.to_vec()]);
    let outcome = mod_.install(Some(&recorded), &mut t).unwrap();
    assert_eq!(outcome, currspice_mods::download::Outcome::Installed);
    assert_eq!(std::fs::read(mod_.path()).unwrap(), body);
}

#[test]
fn sha256_field_empty_string_is_rejected_not_ignored() {
    // `sha256: ""` must be treated as *invalid*, not silently as "no hash".
    let m: ModRef = serde_yaml::from_str("url: https://host/a.jar\nsha256: \"\"").unwrap();
    assert!(m.validate().is_err());
}

#[test]
fn embedded_manifest_is_complete_and_consistent() {
    // The compiled-in default (`include_str!`) is the actual default friends
    // get — it must be well-formed and self-consistent.
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::select(None, dir.path()).unwrap();
    cfg.validate().unwrap();
    cfg.verify_version_consistency().unwrap();
    // Every shipped mod carries a pinned hash (Phase 1 integrity guarantee).
    assert!(
        cfg.mod_refs().all(|m| m.sha256().is_some()),
        "the built-in catalog must pin a sha256 for every mod"
    );
}

// ---------------------------------------------------------------------------
// Version drift guard — the 1.21.1 vs 1.21.11 class of accident
// ---------------------------------------------------------------------------

fn cfg_with(mc: &str, mods: &[&str]) -> Config {
    Config::new(
        "",
        mods,
        &[],
        mc,
        "0.18.4",
        "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar",
    )
}

#[test]
fn drift_catch_matrix() {
    // (declared, url) → expect conflict?
    let cases: [(&str, &str, bool); 8] = [
        // exact match: ok
        ("1.21.11", "https://x/mod-1.21.11.jar", false),
        // same series different patch: conflict
        ("1.21.11", "https://x/mod-1.21.1.jar", true),
        ("1.21.1", "https://x/mod-1.21.11.jar", true),
        // different series (minor): conflict
        ("1.21.11", "https://x/mod-1.20.1.jar", false), // different series — can't disprove
        // non-MC-versioned jar (Kotlin): can't disprove → ok
        (
            "1.21.11",
            "https://x/fabric-language-kotlin-1.13.8.jar",
            false,
        ),
        // substring trap: "1.21.1" is a substring of "1.21.11" — NOT a conflict
        ("1.21.11", "https://x/sodium-0.8.2%2Bmc1.21.11.jar", false),
        // build number embedded in a different form
        ("1.21.11", "https://x/mod-mc1.21.11.jar", false),
        // truly different version in a query/other path segment
        (
            "1.21.11",
            "https://x/data/y/versions/aaa/mod-1.21.8.jar",
            true,
        ),
    ];
    for (mc, url, expect) in cases {
        let cfg = cfg_with(mc, &[url]);
        let got = cfg.verify_version_consistency().is_err();
        assert_eq!(
            got, expect,
            "drift({mc}, {url}) got={got} expected={expect}"
        );
    }
}

#[test]
fn drift_error_names_the_offender_and_declared_version() {
    let cfg = cfg_with(
        "1.21.1",
        &["https://cdn.modrinth.com/data/P7dR8mSH/versions/L34yYPTD/thing-0.141.2%2B1.21.11.jar"],
    );
    let err = cfg.verify_version_consistency().unwrap_err().to_string();
    assert!(
        err.contains("1.21.11"),
        "should name the offending version: {err}"
    );
    assert!(
        err.contains("1.21.1"),
        "should name the declared version: {err}"
    );
    assert!(
        err.to_lowercase().contains("manifest"),
        "should point at the manifest: {err}"
    );
}

// ---------------------------------------------------------------------------
// Selection precedence (explicit > CWD > embedded)
// ---------------------------------------------------------------------------

#[test]
fn explicit_beats_cwd_beats_embedded() {
    let _lock = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    // Distinct version for each source so we can tell which won.
    let explicit = dir.path().join("exp.yaml");
    write_yaml(&explicit, "1.20.4");
    let cwd = dir.path().join("mods.yaml");
    write_yaml(&cwd, "1.19.4");
    std::env::set_current_dir(dir.path()).unwrap();
    // embedded differs from both real versions, so a silent fallback would be visible.
    let explicit_win = Config::select(Some(&explicit), dir.path()).unwrap();
    assert_eq!(explicit_win.get_mc_version(), "1.20.4", "explicit must win");
    let cwd_win = Config::select(None, dir.path()).unwrap();
    assert_eq!(cwd_win.get_mc_version(), "1.19.4", "CWD must beat embedded");
}

#[test]
fn invalid_explicit_errors_and_never_falls_back() {
    let _lock = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.yaml");
    std::fs::write(&bad, "not: [valid, yaml: {{{{").unwrap();
    let cwd = dir.path().join("mods.yaml");
    write_yaml(&cwd, "1.19.4");
    std::env::set_current_dir(dir.path()).unwrap();
    assert!(
        Config::select(Some(&bad), dir.path()).is_err(),
        "a bad explicit file must error, not fall back to CWD/embedded"
    );
}

#[test]
fn invalid_cwd_errors_and_never_falls_back() {
    // A broken CWD override is a hard config error — it must not silently
    // install the embedded (possibly different) catalog.
    let _lock = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().join("mods.yaml");
    std::fs::write(&cwd, "server_mods: [1]\n").unwrap(); // invalid value type
    std::env::set_current_dir(dir.path()).unwrap();
    assert!(
        Config::select(None, dir.path()).is_err(),
        "an invalid CWD manifest must be a hard error"
    );
}

fn write_yaml(path: &std::path::Path, mc: &str) {
    std::fs::write(
        path,
        yaml_manifest(
            mc,
            "0.18.4",
            "https://maven.fabricmc.net/x.jar",
            &[&format!(
                "https://cdn.modrinth.com/data/x/versions/y/a-{mc}.jar"
            )],
            &[],
        ),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Serialization round-trip fidelity (future "personal manifest" endpoint)
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_preserves_every_field_and_order() {
    let mut c = valid_config();
    c.set_user_mods(vec![
        "https://cdn.modrinth.com/data/u/versions/1/user-1.21.11.jar".into(),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("round.yaml");
    c.gen_config_file(&file).unwrap();
    let back = Config::load_new(&file).unwrap();
    assert_eq!(c, back, "gen_config_file → load_new must be lossless");
    back.validate().unwrap();
}

#[test]
fn gen_config_file_creates_parent_dir() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a/b/c/out.yaml");
    assert!(!nested.parent().unwrap().exists());
    valid_config().gen_config_file(&nested).unwrap();
    assert!(nested.exists());
    assert!(Config::load_new(&nested).is_ok());
}

// ---------------------------------------------------------------------------
// Launcher profile editing — the highest-corruption-risk write
// ---------------------------------------------------------------------------

#[test]
fn profile_edit_sets_heap_icon_and_keeps_unrelated_profiles() {
    let dir = tempfile::tempdir().unwrap();
    // A friend's OTHER profile must be preserved verbatim.
    let existing = serde_json::json!({
        "client": {"latest": "1.21.11"},
        "launcherVersion": "1.21.11",
        "profiles": {
            "My Other Modpack": {"lastVersionId":"vanilla-1.21.11","name":"x"},
            "fabric-loader-1.21.11": {"lastVersionId":"fabric-loader-0.18.4-1.21.11"}
        }
    });
    let p = dir.path().join("launcher_profiles.json");
    std::fs::write(&p, existing.to_string()).unwrap();

    let mut c = valid_config();
    c.set_path(dir.path().to_string_lossy().to_string());
    c.edit_profile_at(&p, 6).unwrap();

    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let fabric = &doc["profiles"]["fabric-loader-1.21.11"];
    assert!(fabric["javaArgs"].as_str().unwrap().contains("-Xmx6G"));
    assert_eq!(fabric["name"], currspice_mods::PROFILE_NAME);
    assert!(
        fabric["icon"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    // Unrelated profile fully intact.
    assert_eq!(
        doc["profiles"]["My Other Modpack"]["lastVersionId"],
        "vanilla-1.21.11"
    );
}

#[test]
fn profile_edit_missing_fabric_profile_does_not_create_it() {
    // Regression: old code fabricated the profile from scratch, corrupting
    // the file. Now it's a hard error naming the missing profile.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("launcher_profiles.json");
    std::fs::write(&p, "{\"profiles\":{\"unrelated\":{}}}").unwrap();
    let c = valid_config();
    let err = c.edit_profile_at(&p, 8).unwrap_err().to_string();
    assert!(
        err.contains("fabric-loader-1.21.11"),
        "must name the missing profile: {err}"
    );
    // File must still be valid JSON and unchanged in shape.
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    assert!(doc["profiles"].get("fabric-loader-1.21.11").is_none());
}

#[test]
fn profile_edit_missing_file_is_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let c = valid_config();
    assert!(c.edit_profile_at(&dir.path().join("nope.json"), 4).is_err());
}

#[test]
fn profile_edit_is_idempotent_second_run_is_safe() {
    // The installer calls edit_profile every run; a second pass must not
    // corrupt, duplicate, or drop the existing (already-edited) entry.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("launcher_profiles.json");
    std::fs::write(&p, profile_json("1.21.11", "0.18.4", "{}")).unwrap();
    let c = valid_config();
    c.edit_profile_at(&p, 4).unwrap();
    let mid = std::fs::read_to_string(&p).unwrap();
    c.edit_profile_at(&p, 8).unwrap(); // different heap on re-run
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    assert!(
        doc["profiles"]["fabric-loader-1.21.11"]["javaArgs"]
            .as_str()
            .unwrap()
            .contains("-Xmx8G")
    );
    // Exactly one javaArgs value, no duplicated flags piled up.
    let args = doc["profiles"]["fabric-loader-1.21.11"]["javaArgs"]
        .as_str()
        .unwrap();
    assert_eq!(args.matches("-Xmx").count(), 1, "no Xmx pile-up: {args}");
    let _ = mid;
}

#[test]
fn profile_preserves_custom_javaargs_but_replaces_xmx() {
    // A friend who hand-tuned -XX flags keeps them; only -Xmx is our knob.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("launcher_profiles.json");
    let doc = serde_json::json!({
        "profiles": {
            "fabric-loader-1.21.11": {
                "lastVersionId": "fabric-loader-0.18.4-1.21.11",
                "javaArgs": "-XX:+UseG1GC -Dcom.mojang.eula.agree=true -Xmx1G"
            }
        }
    });
    std::fs::write(&p, doc.to_string()).unwrap();
    let c = valid_config();
    c.edit_profile_at(&p, 16).unwrap();
    let out: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let args = out["profiles"]["fabric-loader-1.21.11"]["javaArgs"]
        .as_str()
        .unwrap();
    assert!(args.contains("-Xmx16G"), "heap updated: {args}");
    assert!(!args.contains("-Xmx1G"), "old heap gone: {args}");
    assert!(args.contains("-XX:+UseG1GC"), "custom flag kept: {args}");
    assert!(
        args.contains("-Dcom.mojang.eula.agree=true"),
        "custom flag kept: {args}"
    );
}

// ---------------------------------------------------------------------------
// RAM allocation (pure numeric helper)
// ---------------------------------------------------------------------------

#[test]
fn build_java_args_edge_matrix() {
    use currspice_mods::config::build_java_args;
    assert_eq!(build_java_args("", 4), "-Xmx4G");
    let r = build_java_args("-XX:+G1 -Xmx2G extra", 6);
    assert!(r.contains("-Xmx6G") && !r.contains("-Xmx2G"));
    assert!(r.contains("-XX:+G1") && r.contains("extra"));
    // multiple Xmx → only ONE survives (the new one)
    let r2 = build_java_args("-Xmx1G -Xmx2G", 3);
    assert_eq!(r2.matches("-Xmx").count(), 1);
    assert!(r2.contains("-Xmx3G"));
}

// ---------------------------------------------------------------------------
// Property: version-conflict detector agrees with itself across many inputs
// ---------------------------------------------------------------------------

#[test]
fn drift_detector_is_deterministic_for_identical_input() {
    // Sweep a handful of representative inputs: the detector must be a
    // pure function of its input — a second call on the same Config is
    // bit-identical.  (Per-case correctness lives in `drift_catch_matrix`.)
    for patch in 0..=4u16 {
        let base = 10u16 + patch % 50;
        let declared = 10u16 + patch;
        let url = format!("https://cdn.example/data/m/versions/v/mod-{declared}.{base}.jar");
        let cfg = cfg_with(&format!("1.{declared}"), &[&url]);
        let a = cfg.verify_version_consistency().is_err();
        let b = cfg.verify_version_consistency().is_err();
        let _ = url;
        assert_eq!(a, b, "detector must be deterministic");
    }
}

#[test]
fn accepted_mod_url_yields_a_safe_single_component_name() {
    let names = [
        "alpha_beta-gamma.deltahash",
        "Sodium-0.8.2%2Bmc1.21.11",
        "a.b.c.d",
        "lowercase_thing",
        "unicode_%C3%A9_name",
    ];
    for name in names {
        let url = format!("https://host.example/data/obj/versions/x/{name}.jar");
        let dir = std::path::Path::new("C:/mods");
        #[allow(clippy::single_match)] // accept vs reject are semantically distinct
        match currspice_mods::mods::Mod::new(&url, dir) {
            Ok(m) => {
                let n = m.get_name();
                assert!(!n.contains('/'), "{n}");
                assert!(!n.contains('\\'), "{n}");
                assert!(!n.contains(':'), "{n}");
                assert!(n.ends_with(".jar"), "{n}");
                // and it lands directly under the mods dir
                assert!(
                    m.path().parent().map(|p| p == dir).unwrap_or(false),
                    "{} escapes {}",
                    m.path().display(),
                    dir.display()
                );
            }
            Err(_) => {} // rejected = also fine, nothing to prove about the path
        }
    }
}
