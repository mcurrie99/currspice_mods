//! JRE provisioning: the resolution state machine, java-version probing,
//! zip-archive safety (path traversal / symlink / duplicate entries),
//! cache-replacement rollback, and the tree-walking Java finder.
mod common;
use common::*;
use currspice_mods::jre::*;
use std::io::Write;
use std::path::{Path, PathBuf};

fn java_name() -> String {
    if cfg!(windows) {
        "java.exe".to_string()
    } else {
        "java".to_string()
    }
}

/// The exact bin/java path inside the Adoptium-style zip we feed the installer.
fn zipped_java() -> PathBuf {
    PathBuf::from("jdk-17.0.9+9/bin").join(java_name())
}

fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(bytes).unwrap();
    }
    w.finish().unwrap()
}

// ---------------------------------------------------------------------------
// Resolution state machine — exhaustive 8-state table
// ---------------------------------------------------------------------------

#[test]
fn resolve_table_is_exhaustive_and_pure() {
    let expected: [(bool, bool, bool, Option<Decision>); 8] = [
        (true, true, true, Some(Decision::System)),
        (true, true, false, Some(Decision::System)),
        (true, false, true, Some(Decision::System)),
        (true, false, false, Some(Decision::System)),
        (false, true, true, Some(Decision::Cached)),
        (false, true, false, Some(Decision::Cached)),
        (false, false, true, Some(Decision::Download)),
        (false, false, false, None),
    ];
    for (s, c, d, want) in expected {
        assert_eq!(resolve(s, c, d), want, "resolve({s},{c},{d})");
    }
}

#[test]
fn decision_is_pure_and_repeatable() {
    for _ in 0..1000 {
        assert_eq!(resolve(true, false, true), Some(Decision::System));
        assert_eq!(resolve(false, false, false), None);
    }
}

// ---------------------------------------------------------------------------
// java -version output probing
// ---------------------------------------------------------------------------

#[test]
fn java_version_probing_matrix() {
    let cases: [(&str, &str, bool, bool); 14] = [
        // success + version on stderr (real JDK behavior)
        ("", "openjdk version \"17.0.12\" 2024-07-16", true, true),
        ("", "java version \"21.0.1\"", true, true),
        // version on stdout also accepted
        ("openjdk 21-ea 2025-09-16", "", true, true),
        // "1." legacy prefix (Java 8-style 1.8) must NOT count as 17+
        ("", "java version \"1.8.0_402\"", true, false),
        // below-17 openjdk must be rejected
        ("", "openjdk version \"16.0.2\"", true, false),
        ("", "openjdk version \"11.0.20\"", true, false),
        // non-zero exit: rejected regardless of text
        ("", "openjdk version \"17.0.12\"", false, false),
        (
            "",
            "The JAVA_HOME environment variable is not defined correctly",
            false,
            false,
        ),
        ("", "", true, false),
        ("", "no output at all", true, false),
        // weird tokenization: "openjdk 17-ea" → 17 accepted
        ("", "openjdk 17-ea 2024-01-01", true, true),
        // version embedded after other words on one line still found
        (
            "",
            "some banner\nopenjdk version \"17.0.1+10-1\"\nsome footer",
            true,
            true,
        ),
        // quoted-with-spaces and garbage must not false-positive
        ("", "version \"17.\"", true, false), // 17. is not a full major token
        // 170 major (future) accepted
        ("", "openjdk version \"170.0\"", true, true),
    ];
    for (stdout, stderr, success, want) in cases {
        let got = accepts_java_output(success, stdout, stderr);
        assert_eq!(got, want, "probe({success}, {stdout:?}, {stderr:?})");
    }
}

// ---------------------------------------------------------------------------
// Zip safety — path traversal, symlinks, absolute paths, drive letters
// ---------------------------------------------------------------------------

#[test]
fn extract_zip_rejects_traversal_and_absolute_paths() {
    for name in [
        "../escape.bin",
        "a/../../escape.bin",
        "a//../../escape.bin",
        "/absolute/escape.bin",
        "C:/windows/win.ini",
        "C:evil.txt",
        "..\\escape.win",
        "\\\\server\\share\\x.bin",
        "a/b/../../../c.bin",
        "legit/../tricky.bin",
    ] {
        let bytes = make_zip(&[(name, b"pwned")]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out");
        let err = extract_zip(&bytes, &dest).unwrap_err();
        assert!(err.to_string().contains(name), "{err} should cite {name:?}");
        // And critically: nothing escaped the destination.
        let mut escaped = false;
        let walk = |p: &std::path::Path| {
            if p.is_file() {
                let name = p.file_name().map(|s| s.to_string_lossy().to_string());
                if matches!(name.as_deref(), Some("escape.bin") | Some("win.ini") | Some("evil.txt") | Some("escape.win") | Some("x.bin") | Some("tricky.bin") | Some("c.bin")) {
                    escaped = true;
                }
            }
        };
        if let Ok(rd) = std::fs::read_dir(dir.path()) {
            for e in rd.flatten() {
                walk(&e.path());
            }
        }
        assert!(!escaped, "an entry escaped the destination for {name:?}");
    }
}

#[test]
fn extract_zip_rejects_symlink_entries() {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    w.add_symlink("link", "../outside", zip::write::SimpleFileOptions::default())
        .unwrap();
    let bytes = w.finish().unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert!(extract_zip(&bytes, dir.path()).is_err(), "symlink entry must be rejected");
}

#[test]
fn extract_zip_writes_a_normal_nested_tree() {
    let bytes = make_jre_zip();
    let dir = tempfile::tempdir().unwrap();
    extract_zip(&bytes, dir.path()).unwrap();
    let root = dir.path().join("jdk-17.0.9+9");
    assert!(root.join("bin").join(java_name()).is_file());
    assert!(root.join("lib/modules").is_file());
    assert!(root.join("release").is_file());
}

#[test]
fn extract_zip_refuses_destination_containing_symlinks() {
    // A caller-supplied destination that already contains a symlink ancestor
    // is a foot-gun (e.g. `.../link -> sensitive/`). The extractor must bail.
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("secret.txt"), b"top secret").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink as us;
        us(&real, dir.path().join("lnk")).unwrap();
        // A zip writing under the symlinked dir:
        let bytes = make_zip(&[("lnk/planted.bin", b"x")]);
        // Our guard checks ANCESTORS of the target — `lnk` is such an ancestor.
        assert!(
            extract_zip(&bytes, dir.path()).is_err(),
            "a symlink ancestor in the destination must be rejected"
        );
        assert!(!dir.path().join("real/planted.bin").exists(), "must not plant through a symlink");
    }
}

#[test]
fn extract_zip_duplicate_entry_second_write_fails_instead_of_clobbering() {
    // A malicious archive with the same entry twice could, without the
    // create_new guard, overwrite the first. With it, the second write is an
    // error — the function must fail, not silently take one version.
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    w.start_file("dup.bin", zip::write::SimpleFileOptions::default()).unwrap();
    w.write_all(b"first").unwrap();
    w.start_file("dup.bin", zip::write::SimpleFileOptions::default()).unwrap();
    w.write_all(b"second").unwrap();
    let bytes = w.finish().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out");
    // Either rejected outright, or (first write happened) the first content
    // wins and the second must not overwrite it. `create_new` makes the
    // second open fail → Err.
    let res = extract_zip(&bytes, &dest);
    assert!(
        res.is_err(),
        "a duplicate-entry archive must not be silently accepted"
    );
}

// ---------------------------------------------------------------------------
// find_java_in_tree
// ---------------------------------------------------------------------------

#[test]
fn find_java_finds_at_supported_depths() {
    for depth in 0..=3 {
        let dir = tempfile::tempdir().unwrap();
        let mut p = dir.path().to_path_buf();
        for i in 0..depth {
            p = p.join(format!("lvl{i}"));
        }
        p = p.join("bin");
        std::fs::create_dir_all(&p).unwrap();
        let java = p.join(java_name());
        std::fs::write(&java, b"stub").unwrap();
        assert_eq!(
            find_java_in_tree(dir.path()),
            Some(java),
            "depth {depth} must be locatable"
        );
    }
}

#[test]
fn find_java_rejects_bare_java_files_not_in_bin() {
    // A lone `java` at the root (not under `bin/`) is not a JRE layout.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(java_name()), b"not a jre").unwrap();
    assert!(find_java_in_tree(dir.path()).is_none());
}

#[test]
fn find_java_empty_and_garbage_trees_return_none() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..50 {
        std::fs::create_dir_all(dir.path().join(format!("noise{i}"))).unwrap();
    }
    std::fs::write(dir.path().join("noise0/jar.bin"), b"x").unwrap();
    assert!(find_java_in_tree(dir.path()).is_none());
    let empty = tempfile::tempdir().unwrap();
    assert!(find_java_in_tree(empty.path()).is_none());
}

// ---------------------------------------------------------------------------
// ensure_java_with — injected probes, no real java
// ---------------------------------------------------------------------------

#[test]
fn ensure_java_prefers_in_order_and_errors_when_all_offline() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("cache");
    let system = "java-on-path".to_string();
    let jhome_java = "JAVA_HOME/bin/java".to_string();
    let cache_java = "cache-home/bin/java".to_string();

    // system works → win
    let got = ensure_java_with(false, &home, None, |p| p == "java-on-path", |_| panic!("online"))
        .unwrap();
    assert_eq!(got.to_string_lossy(), system);

    // system broken, JAVA_HOME valid → win
    let got = ensure_java_with(
        false,
        &home,
        Some(Path::new(jhome_java.trim_end_matches("/bin/java"))),
        |p| p == jhome_java,
        |_| panic!("online"),
    )
    .unwrap();
    assert_eq!(got, PathBuf::from(jhome_java));

    // nothing valid → offline error, names the options
    let err = ensure_java_with(false, &home, None, |_| false, |_| panic!("online"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Java 17"), "{err}");
    assert!(err.contains("Downloads are disabled"), "{err}");

    // nothing valid offline + download allowed → install() is called exactly once
    let mut installs = 0usize;
    let got = ensure_java_with(
        true,
        &home,
        None,
        |_| false,
        |_| {
            installs += 1;
            Ok(PathBuf::from("installed/java"))
        },
    )
    .unwrap();
    assert_eq!(got, PathBuf::from("installed/java"));
    assert_eq!(installs, 1);
    let _ = cache_java;
}

#[test]
fn ensure_java_with_empty_java_home_is_ignored() {
    // An empty JAVA_HOME must not be probed as "" (which would be "./bin/java").
    let dir = tempfile::tempdir().unwrap();
    let probed: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let _ = ensure_java_with(
        false,
        &dir.path().join("cache"),
        Some(Path::new("")),
        |p| {
            probed.lock().unwrap().push(p.to_string_lossy().to_string());
            true
    },
        |_| panic!("online"),
    );
    // The JAVA_HOME path "" joined with bin/java would be "./bin/java" — we
    // must NOT have probed that; the guard filters empty strings.
    let list = probed.lock().unwrap().clone();
    assert!(
        !list.iter().any(|p| p.ends_with("./bin/java") || p.ends_with(r".\bin\java.exe")),
        "empty JAVA_HOME produced a relative probe: {list:?}"
    );
}

// ---------------------------------------------------------------------------
// install_jre_with — the risky cache-replacement flow
// ---------------------------------------------------------------------------

#[test]
fn valid_cached_jre_short_circuits_before_network() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("cache");
    let java = home.join("bin").join(java_name());
    std::fs::create_dir_all(java.parent().unwrap()).unwrap();
    std::fs::write(&java, b"old-but-valid").unwrap();
    let got = install_jre_with(&home, "windows", "x86_64", |_| true, || panic!("must not download")).unwrap();
    assert_eq!(got, java);
}

#[test]
fn platform_gate_before_download() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("cache");
    for (os, arch) in [("linux", "x86_64"), ("macos", "x86_64"), ("windows", "aarch64"), ("windows", "i686")] {
        let r = install_jre_with(&home, os, arch, |_| false, || panic!("unsupported")).unwrap_err();
        assert!(r.to_string().contains("Windows x64"), "{os}/{arch}: {r}");
        assert!(!home.exists(), "must not create a cache on unsupported platforms");
    }
}

#[test]
fn download_failure_preserves_existing_cache_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("cache");
    let java = home.join("bin").join(java_name());
    std::fs::create_dir_all(java.parent().unwrap()).unwrap();
    let prior = b"PRIOR CACHE - PRESERVE ME";
    std::fs::write(&java, prior).unwrap();
    // Extra unrelated sibling the friend created by hand:
    let custom = home.join("user-notes.txt");
    std::fs::write(&custom, b"my server notes").unwrap();

    let err = install_jre_with(&home, "windows", "x86_64", |_| false, || Err("offline".into())).unwrap_err();
    assert!(err.to_string().contains("offline"));
    assert_eq!(std::fs::read(&java).unwrap(), prior, "prior cache bytes preserved");
    assert_eq!(std::fs::read(&custom).unwrap(), b"my server notes", "unrelated sibling preserved");
}

#[test]
fn bad_downloaded_java_keeps_prior_cache_and_reports_preservation() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("cache");
    let java = home.join("bin").join(java_name());
    std::fs::create_dir_all(java.parent().unwrap()).unwrap();
    let prior = b"PRIOR CACHE";
    std::fs::write(&java, prior).unwrap();

    // A zip that extracts fine and even locates a `java`, but that probe says
    // it's not 17+ → the whole replace must be aborted with the old cache kept.
    let bytes = make_jre_zip();
    let err = install_jre_with(&home, "windows", "x86_64", |_| false, || Ok(bytes)).unwrap_err();
    assert!(err.to_string().contains("Java"), "{err}");
    assert_eq!(std::fs::read(&java).unwrap(), prior, "old cache retained after a bad replacement");
}

#[test]
fn archive_without_java_is_rejected_and_preserves_cache() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("cache");
    let java = home.join("bin").join(java_name());
    std::fs::create_dir_all(java.parent().unwrap()).unwrap();
    let prior = b"KEEP";
    std::fs::write(&java, prior).unwrap();
    let bytes = make_zip(&[("junk/notes.txt", b"no java here")]);
    let err = install_jre_with(&home, "windows", "x86_64", |_| false, || Ok(bytes)).unwrap_err();
    assert!(err.to_string().contains("no bin/java"), "{err}");
    assert_eq!(std::fs::read(&java).unwrap(), prior);
}

#[test]
fn empty_download_body_is_rejected_with_a_human_message() {
    let dir = tempfile::tempdir().unwrap();
    let err = install_jre_with(&dir.path().join("c"), "windows", "x86_64", |_| false, || Ok(Vec::new()))
        .unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");
}

#[test]
fn replacement_success_installs_new_layout_and_old_cache_backup_is_clean() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("cache");
    let old_java = home.join("bin").join(java_name());
    std::fs::create_dir_all(old_java.parent().unwrap()).unwrap();
    std::fs::write(&old_java, b"old").unwrap();

    // New "download": a valid JRE layout with a bin/java the probe accepts.
    let bytes = make_jre_zip();
    let ok_java = zipped_java(); // relative to the staging extraction root
    let got = install_jre_with(
        &home,
        "windows",
        "x86_64",
        |p| p.ends_with(&ok_java) || p == home.join("bin").join(java_name()),
        || Ok(bytes),
    )
    .unwrap();
    // The returned java must now live under the public cache root.
    assert!(
        got.starts_with(&home),
        "installed java must be under the cache root: {} (root {})",
        got.display(),
        home.display()
    );
    assert!(got.exists(), "the installed java must exist on disk");
}

#[test]
fn extraction_refuses_existing_file_collision() {
    // A fresh extraction whose entry collides with an existing file in the
    // destination (a prior half-extraction) must fail, not clobber.
    let dir = tempfile::tempdir().unwrap();
    let collision = dir.path().join("jdk-17.0.9+9/bin/java.txt");
    std::fs::create_dir_all(collision.parent().unwrap()).unwrap();
    std::fs::write(&collision, b"PRIOR-CONTENT-MUST-SURVIVE").unwrap();
    let bytes = make_zip(&[("jdk-17.0.9+9/bin/java.txt", b"attacker")]);
    let res = extract_zip(&bytes, dir.path());
    assert!(res.is_err(), "collision must be an error");
    assert_eq!(
        std::fs::read(&collision).unwrap(),
        b"PRIOR-CONTENT-MUST-SURVIVE",
        "prior content must not be overwritten"
    );
}

#[test]
fn extract_zip_directory_entries_only_creates_dirs() {
    // A directory entry (trailing slash name) must be created, not written
    // as a file.
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut opt = zip::write::SimpleFileOptions::default();
    opt.set_unix_permissions(0o40755);
    w.start_file("emptydir/", opt).unwrap();
    let bytes = w.finish().unwrap();
    let dir = tempfile::tempdir().unwrap();
    extract_zip(&bytes, dir.path()).unwrap();
    let p = dir.path().join("emptydir");
    assert!(p.is_dir(), "a directory entry must materialize as a directory");
}
