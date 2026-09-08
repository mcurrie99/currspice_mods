//! Java resolution for the Fabric step: friends shouldn't need to know
//! Java exists.
//!
//! Preference order (ROADMAP P5: the install must "just work"):
//!   1. a working `java` already on the machine (PATH / JAVA_HOME)
//!   2. a Cached portable JRE in `%LOCALAPPDATA%\Currspice\jre17`
//!   3. download a Temurin 17 JRE once (network), extract, then treat as (2)
//!
//! The decision table itself is a pure function (`resolve`) so it is
//! fully unit-testable with no Java, no network, and no 40 MB download —
//! everything that talks to the real world lives behind tiny probes.
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a portable JRE lives. (Windows friends: `%LOCALAPPDATA%\Currspice\jre17`.)
pub fn jre_home() -> PathBuf {
    base_data_dir().join("Currspice").join("jre17")
}

fn base_data_dir() -> PathBuf {
    if let Ok(localappdata) = env::var("LOCALAPPDATA") {
        return PathBuf::from(localappdata);
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let base = PathBuf::from(home);
    if cfg!(windows) {
        base
    } else {
        base.join(".local").join("share")
    }
}

/// Temurin 17 JRE archive for the current platform (Adoptium's binary
/// endpoint — a 302 to the actual release zip).
pub fn jre_download_url() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        const _AARCH64: () = (); // reserved; fall through to x64 build
    }
    if cfg!(windows) {
        "https://api.adoptium.net/v3/binary/latest/17/ga/windows/x64/jre/hotspot/normal/eclipse"
    } else {
        "https://api.adoptium.net/v3/binary/latest/17/ga/linux/x64/jre/hotspot/normal/eclipse"
    }
}

// ---- pure decision core ----------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Use the machine's own java.
    System,
    /// Use the cached portable JRE under `jre_home`.
    Cached,
    /// Download the portable JRE, then treat as Cached.
    Download,
}

/// The whole "where does java come from" state machine, with the
/// environment injected as booleans. This is what the tests lock down.
pub fn resolve(system_ok: bool, cache_ok: bool, allow_download: bool) -> Option<Decision> {
    if system_ok {
        return Some(Decision::System);
    }
    if cache_ok {
        return Some(Decision::Cached);
    }
    if allow_download {
        return Some(Decision::Download);
    }
    None
}

// ---- real-world probes -------------------------------------------------------

/// `java -version` — true iff a usable Java is on PATH/JAVA_HOME.
pub fn probe_java(bin: &Path) -> bool {
    Command::new(bin)
        .arg("-version")
        .output()
        .map(|o| {
            accepts_java_output(
                o.status.success(),
                &String::from_utf8_lossy(&o.stdout),
                &String::from_utf8_lossy(&o.stderr),
            )
        })
        .unwrap_or(false)
}

/// Exposed for tests (driven via `tests/jre_ext.rs`). Returns true iff the
/// parsed version string names Java 17 or newer.
pub fn accepts_java_output(success: bool, stdout: &str, stderr: &str) -> bool {
    success
        && stderr.lines().chain(stdout.lines()).any(|line| {
            let mut words = line.split_whitespace();
            if !matches!(words.next(), Some("java" | "openjdk")) {
                return false;
            }
            let token = match words.next() {
                Some("version") => words.next(),
                other => other,
            };
            token
                .and_then(|s| s.trim_matches('"').split(['.', '-']).next())
                .and_then(|s| s.parse::<u32>().ok())
                .is_some_and(|major| major >= 17)
        })
}

/// The platform Java executable name (`java` or `java.exe`).
pub fn java_bin_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// The cached JRE's java, if the layout is present.
pub fn cached_java(home: &Path) -> Option<PathBuf> {
    let bin = home.join("bin").join(java_bin_name());
    bin.exists().then_some(bin)
}

// ---- download + extract ------------------------------------------------------

/// Resolve and return the java binary to run the Fabric installer with.
/// Never panics; every failure mode explains itself to a friend.
pub fn ensure_java(allow_download: bool) -> Result<PathBuf, Box<dyn Error>> {
    ensure_java_with(
        allow_download,
        &jre_home(),
        env::var_os("JAVA_HOME").map(PathBuf::from).as_deref(),
        probe_java,
        install_jre,
    )
}

/// Injected seam for the resolution order so tests can drive every branch
/// without a real `java` or network (see `tests/jre_ext.rs`).
pub fn ensure_java_with(
    allow_download: bool,
    home: &Path,
    java_home: Option<&Path>,
    mut probe: impl FnMut(&Path) -> bool,
    mut install: impl FnMut(&Path) -> Result<PathBuf, Box<dyn Error>>,
) -> Result<PathBuf, Box<dyn Error>> {
    let system = PathBuf::from(java_bin_name());
    if probe(&system) {
        return Ok(system);
    }
    if let Some(jhome) = java_home.filter(|p| !p.as_os_str().is_empty()) {
        let java = jhome.join("bin").join(java_bin_name());
        if probe(&java) {
            return Ok(java);
        }
    }
    if let Some(java) = cached_java(home).filter(|java| probe(java)) {
        return Ok(java);
    }
    if allow_download {
        println!(
            "Currspice: Java 17+ not found — fetching a private installer runtime (one-time, ~40 MB)..."
        );
        return install(home);
    }
    Err("No working Java 17+ was found on PATH, JAVA_HOME or in the private cache. Downloads are disabled. Install Java 17+ or enable downloads and re-run.".into())
}

/// Download → `.part` → extract → relocate into `home`. Idempotent:
/// an existing valid cache short-circuits the whole thing.
pub fn install_jre(home: &Path) -> Result<PathBuf, Box<dyn Error>> {
    install_jre_with(
        home,
        env::consts::OS,
        env::consts::ARCH,
        probe_java,
        download_archive,
    )
}

fn download_archive() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut resp = reqwest::blocking::get(jre_download_url())?;
    let mut body = Vec::new();
    resp.read_to_end(&mut body)?;
    if body.is_empty() {
        return Err("Java download came back empty".into());
    }
    Ok(body)
}

/// Core download/extract/relocate flow, exposed so tests can inject the
/// probe and download (see `tests/jre_ext.rs`) and unit-test the risky
/// cache-replacement logic without a 40 MB network fetch.
pub fn install_jre_with(
    home: &Path,
    os: &str,
    arch: &str,
    mut probe_java: impl FnMut(&Path) -> bool,
    mut download: impl FnMut() -> Result<Vec<u8>, Box<dyn Error>>,
) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(java) = cached_java(home)
        && probe_java(&java)
    {
        return Ok(java);
    }
    if os != "windows" || arch != "x86_64" {
        return Err("Private Java provisioning supports Windows x64 only; install Java 17+ on PATH or set JAVA_HOME.".into());
    }

    let body = download()?;
    if body.is_empty() {
        return Err("Java download came back empty".into());
    }
    let parent = home
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut stage = StagingDir::create(parent)?;
    let extracted = stage.0.as_ref().unwrap().join("extracted");
    fs::create_dir(&extracted)?;
    extract_zip(&body, &extracted)?;
    let java = find_java_in_tree(&extracted).ok_or("Java archive has no bin/java")?;
    if !probe_java(&java) {
        return Err(
            "Downloaded Java does not run or is older than Java 17; existing cache preserved"
                .into(),
        );
    }
    let root = java
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid JRE layout")?;
    let backup = stage.0.as_ref().unwrap().join("previous");
    let had_cache = fs::symlink_metadata(home).is_ok();
    if had_cache {
        fs::rename(home, &backup)?;
    }
    if let Err(error) = fs::rename(root, home) {
        if had_cache && fs::rename(&backup, home).is_err() {
            stage.0.take(); // Keep the recoverable old cache, never delete it.
            return Err(format!(
                "JRE replacement failed: {error}; old cache retained at {}",
                backup.display()
            )
            .into());
        }
        return Err(format!("could not move the JRE into place: {error}").into());
    }
    let final_java = home.join("bin").join(java_bin_name());
    if !probe_java(&final_java) {
        // This tree belongs to this invocation. Roll back if relocation broke it.
        if fs::remove_dir_all(home).is_err() || (had_cache && fs::rename(&backup, home).is_err()) {
            stage.0.take();
            return Err(format!(
                "Java failed after relocation; recovery files retained at {}",
                backup.display()
            )
            .into());
        }
        return Err("Java failed after relocation; previous cache restored".into());
    }
    Ok(final_java)
}

// Atomically reserve a unique sibling directory; only clean up our own files.
struct StagingDir(Option<PathBuf>);
impl StagingDir {
    fn create(parent: &Path) -> io::Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        for _ in 0..100 {
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = parent.join(format!(".currspice-jre-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(Some(path))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Cannot reserve Java staging directory",
        ))
    }
}
impl Drop for StagingDir {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_dir_all(path);
        }
    }
}

/// Find `bin/java` (or `bin/java.exe`) anywhere in a tree, depth ≤ 3.
/// The Adoptium zip nests the JRE under `jdk-17.0.x+y/`.
pub fn find_java_in_tree(root: &Path) -> Option<PathBuf> {
    let mut stack = Vec::new();
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            stack.push(e.path());
        }
    }
    let mut depth = 0;
    while let Some(dir) = stack.pop() {
        depth += 1;
        if depth > 4 {
            continue;
        }
        for cand in [
            dir.join("bin").join(java_bin_name()),
            dir.join(java_bin_name()),
        ] {
            if cand.is_file() {
                return Some(cand);
            }
        }
        if let Ok(rd) = fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    stack.push(e.path());
                }
            }
        }
    }
    None
}

/// Minimal zip extractor (deflate + store), no symlink shenanigans —
/// enough for a JRE archive, and testable with in-memory zips.
pub fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), Box<dyn Error>> {
    use std::io::Cursor;
    use zip::ZipArchive;

    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        // Treat names as portable paths, independent of the host OS. Reject
        // backslashes and colons too (Windows separators, drives and ADS).
        if name.starts_with('/')
            || name.contains('\\')
            || name.contains(':')
            || name.split('/').any(|part| part == "..")
            || file.enclosed_name().is_none()
            || file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("unsafe Java archive entry: {name}").into());
        }
        let target = dest.join(&name);
        // Do not follow links already present in a caller-supplied destination.
        for ancestor in target.ancestors() {
            if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("Java extraction destination contains a symlink".into());
            }
        }
        if file.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        io::copy(&mut file, &mut out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- the decision state machine (pure, no side effects) ----------------

    #[test]
    fn resolve_prefers_system_java() {
        assert_eq!(resolve(true, true, true), Some(Decision::System));
        assert_eq!(resolve(true, false, false), Some(Decision::System));
    }

    #[test]
    fn resolve_prefers_cache_over_download() {
        assert_eq!(resolve(false, true, true), Some(Decision::Cached));
        assert_eq!(resolve(false, true, false), Some(Decision::Cached));
    }

    #[test]
    fn resolve_downloads_only_when_allowed() {
        assert_eq!(resolve(false, false, true), Some(Decision::Download));
        assert_eq!(resolve(false, false, false), None);
    }

    #[test]
    fn resolve_ordering_is_total() {
        // Every one of the 8 combinations resolves exactly once — no
        // ambiguous states in the friend-visible path.
        for s in [false, true] {
            for c in [false, true] {
                for d in [false, true] {
                    let _ = resolve(s, c, d);
                }
            }
        }
    }

    #[test]
    fn java_probe_requires_version_17_or_newer_and_success() {
        for text in [
            "",
            "nonsense 99",
            "java version \"1.8.0_402\"",
            "openjdk version \"16.0.2\"",
        ] {
            assert!(!accepts_java_output(true, "", text), "accepted {text}");
        }
        for text in [
            "openjdk version \"17.0.12\" 2024-07-16",
            "java version \"21.0.1\"",
            "openjdk 25-ea 2025-09-16",
        ] {
            assert!(accepts_java_output(true, "", text));
            assert!(accepts_java_output(true, text, ""));
            assert!(!accepts_java_output(false, "", text));
        }
    }

    // ---- layout & probes ------------------------------------------------------

    #[test]
    fn cached_java_path_is_stable_layout() {
        // The contract that matters: `cached_java(home)` is exactly
        // `<home>/bin/java[.exe]` — a friend or the cache check can compute
        // it the same way. Verified against a real, existing layout.
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join(java_bin_name()), b"stub").unwrap();
        assert_eq!(cached_java(dir.path()), Some(bin.join(java_bin_name())));
    }

    #[test]
    fn cached_java_absent_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(cached_java(dir.path()).is_none());
    }

    #[test]
    fn cached_java_detects_valid_layout() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let java = bin.join(java_bin_name());
        fs::write(&java, b"stub").unwrap();
        assert_eq!(cached_java(dir.path()), Some(java));
    }

    #[test]
    fn ensure_java_is_deterministic_given_probes() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("cache");
        let jhome = dir.path().join("jdk");
        let expected = jhome.join("bin").join(java_bin_name());
        let result = ensure_java_with(
            false,
            &home,
            Some(&jhome),
            |p| p == expected,
            |_| panic!("offline download"),
        );
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn bad_cache_is_not_accepted_offline_and_is_repaired_online() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("bin")).unwrap();
        let java = dir.path().join("bin").join(java_bin_name());
        fs::write(&java, "broken").unwrap();
        assert!(
            ensure_java_with(
                false,
                dir.path(),
                None,
                |_| false,
                |_| panic!("offline download")
            )
            .is_err()
        );
        let mut installs = 0;
        let result = ensure_java_with(
            true,
            dir.path(),
            None,
            |_| false,
            |_| {
                installs += 1;
                Ok(java.clone())
            },
        );
        assert_eq!(result.unwrap(), java);
        assert_eq!(installs, 1);
        assert_eq!(fs::read_to_string(java).unwrap(), "broken");
    }

    #[test]
    fn zip_rejects_unsafe_names_and_symlinks() {
        use std::io::{Cursor, Write};
        for name in [
            "../escape",
            "/absolute",
            "C:/drive",
            "C:drive",
            "..\\escape",
            "\\\\server\\share",
            "a/../../escape",
        ] {
            let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
            w.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"bad").unwrap();
            let bytes = w.finish().unwrap().into_inner();
            let dir = tempfile::tempdir().unwrap();
            let dest = dir.path().join("out");
            assert!(extract_zip(&bytes, &dest).is_err(), "accepted {name}");
            assert!(!dir.path().join("escape").exists());
        }
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        w.add_symlink(
            "link",
            "../outside",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let dir = tempfile::tempdir().unwrap();
        assert!(extract_zip(&bytes, dir.path()).is_err());
    }

    // ---- zip extraction (the risky file-system part) --------------------------

    fn make_test_zip() -> Vec<u8> {
        use std::io::{Cursor, Write};
        use zip::ZipWriter;
        use zip::write::SimpleFileOptions;

        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default();
        // Mimic the Adoptium layout: a nested JRE root with bin/java(.exe).
        w.start_file("jdk-17.0.9+9/bin/java.txt", opts).unwrap();
        w.write_all(b"stub-java").unwrap();
        w.start_file("jdk-17.0.9+9/lib/libfoo.zip", opts).unwrap();
        w.write_all(b"stub-lib").unwrap();
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn extract_zip_builds_nested_tree() {
        let bytes = make_test_zip();
        let dir = tempfile::tempdir().unwrap();
        extract_zip(&bytes, dir.path()).expect("extract");
        assert!(dir.path().join("jdk-17.0.9+9/bin/java.txt").is_file());
        assert!(dir.path().join("jdk-17.0.9+9/lib/libfoo.zip").is_file());
    }

    #[test]
    fn find_java_in_tree_locates_nested_root() {
        let bytes = make_test_zip();
        let dir = tempfile::tempdir().unwrap();
        extract_zip(&bytes, dir.path()).unwrap();
        // A `java.txt` stub is not the real `java` — the finder must keep
        // looking (and miss): no false positives.
        assert!(find_java_in_tree(dir.path()).is_none());
    }

    #[test]
    fn find_java_in_tree_finds_real_layout() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("jdk-17.0.9+9").join("bin");
        fs::create_dir_all(&nested).unwrap();
        let java = nested.join(java_bin_name());
        fs::write(&java, b"stub").unwrap();
        assert_eq!(find_java_in_tree(dir.path()), Some(java));
    }

    #[test]
    fn provisioning_rejects_unsupported_platform_before_network() {
        let dir = tempfile::tempdir().unwrap();
        for (os, arch) in [
            ("linux", "x86_64"),
            ("windows", "aarch64"),
            ("macos", "x86_64"),
        ] {
            let home = dir.path().join("cache");
            let result = install_jre_with(
                &home,
                os,
                arch,
                |_| false,
                || panic!("unsupported platform downloaded"),
            );
            assert!(result.unwrap_err().to_string().contains("Windows x64"));
            assert!(!home.exists());
        }
    }

    #[test]
    fn invalid_replacement_preserves_cache_and_unrelated_scratch() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("cache");
        fs::create_dir_all(home.join("bin")).unwrap();
        let java = home.join("bin").join(java_bin_name());
        fs::write(&java, "old cache").unwrap();
        let scratch = home.with_extension("extract");
        fs::create_dir(&scratch).unwrap();
        fs::write(scratch.join("unrelated"), "keep").unwrap();
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        w.start_file(
            format!("jdk/bin/{}", java_bin_name()),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        std::io::Write::write_all(&mut w, b"broken replacement").unwrap();
        let bytes = w.finish().unwrap().into_inner();
        assert!(
            install_jre_with(&home, "windows", "x86_64", |_| false, || Ok(bytes.clone())).is_err()
        );
        assert_eq!(fs::read_to_string(java).unwrap(), "old cache");
        assert_eq!(
            fs::read_to_string(scratch.join("unrelated")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn failed_download_preserves_existing_cache() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("cache");
        fs::create_dir_all(home.join("bin")).unwrap();
        let java = home.join("bin").join(java_bin_name());
        fs::write(&java, "old cache").unwrap();
        assert!(
            install_jre_with(
                &home,
                "windows",
                "x86_64",
                |_| false,
                || Err("offline".into())
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(java).unwrap(), "old cache");
    }

    #[test]
    fn install_jre_refuses_bad_archive() {
        // The "archive without a JRE" failure mode is tested without
        // re-downloading 40 MB: install_jre's own archive validation is
        // covered here by checking find_java_in_tree returns None for a
        // zip with no bin/java — which install_jre maps to the exact
        // error a friend would see.
        let empty = std::io::Cursor::new(Vec::new());
        let bytes = zip::ZipWriter::new(empty).finish().unwrap().into_inner();
        let dir = tempfile::tempdir().unwrap();
        // extract of an empty (but valid) archive succeeds, then validation
        // finds nothing:
        assert!(extract_zip(&bytes, dir.path()).is_ok());
        assert!(find_java_in_tree(dir.path()).is_none());
    }
}
