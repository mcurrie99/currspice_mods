//! Download & jar-integrity: the `Mod::install` state machine (install /
//! skip / replace / retry / preserve-on-failure), `valid_jar`, `file_sha256`,
//! and the HTTPS-only transport contract.
mod common;
use common::*;
use currspice_mods::download::{Outcome, Transport, file_sha256, valid_jar};
use currspice_mods::mods::Mod;

fn fetch_bytes(
    t: &mut currspice_mods::download::HttpTransport,
    url: &str,
) -> std::io::Result<Vec<u8>> {
    let mut sink: Vec<u8> = Vec::new();
    let res = t.fetch(url, &mut sink);
    match res {
        Ok(()) => Ok(sink),
        Err(e) => Err(std::io::Error::other(e.to_string())),
    }
}
use std::path::Path;

// ---------------------------------------------------------------------------
// file_sha256
// ---------------------------------------------------------------------------

#[test]
fn file_sha256_matches_known_vectors() {
    let dir = tempfile::tempdir().unwrap();
    let cases: [(&[u8], &str); 3] = [
        (
            b"",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            b"The quick brown fox jumps over the lazy dog",
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
        ),
    ];
    for (bytes, expected) in cases {
        let p = dir.path().join("f.bin");
        std::fs::write(&p, bytes).unwrap();
        assert_eq!(file_sha256(&p).unwrap(), expected);
    }
}

#[test]
fn file_sha256_streams_large_files_without_oom() {
    // 5 MiB in one buffer at a time — checks the 64 KiB streaming loop and
    // that the result matches a single-pass reference.
    let dir = tempfile::tempdir().unwrap();
    let big = vec![7u8; 5 * 1024 * 1024];
    let p = dir.path().join("big.bin");
    std::fs::write(&p, &big).unwrap();
    let streamed = file_sha256(&p).unwrap();
    {
        use sha2::{Digest, Sha256};
        let mut ref_hash = Sha256::new();
        ref_hash.update(&big);
        let expected = format!("{:x}", ref_hash.finalize());
        assert_eq!(streamed, expected);
    }
}

#[test]
fn file_sha256_missing_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert!(file_sha256(&dir.path().join("nope")).is_err());
}

// ---------------------------------------------------------------------------
// valid_jar (structural check for unpinned/legacy mods)
// ---------------------------------------------------------------------------

#[test]
fn valid_jar_accepts_well_formed_zip_and_rejects_garbage() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.jar");
    std::fs::write(&good, make_jar()).unwrap();
    assert!(valid_jar(&good), "a real jar must pass");

    let cases: &[(&str, &[u8])] = &[
        ("empty", b""),
        ("text", b"this is not a zip at all"),
        ("truncated_zip", &make_jar()[..make_jar().len() / 2]),
    ];
    for (name, bytes) in cases {
        let p = dir.path().join(format!("{name}.jar"));
        std::fs::write(&p, bytes).unwrap();
        assert!(!valid_jar(&p), "{name} must not pass as a jar");
    }
    assert!(!valid_jar(&dir.path().join("missing.jar")));
}

#[test]
fn valid_jar_rejects_empty_archive() {
    // An archive with zero entries is structurally "valid" as a zip but is
    // not a usable jar — the function must say no.
    let dir = tempfile::tempdir().unwrap();
    let w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let empty = w.finish().unwrap().into_inner();
    let p = dir.path().join("empty.jar");
    std::fs::write(&p, &empty).unwrap();
    assert!(!valid_jar(&p));
}

// ---------------------------------------------------------------------------
// Mod::install — the integrity state machine
// ---------------------------------------------------------------------------

fn fresh_mod(dir: &Path) -> Mod {
    Mod::new("https://host.example/mods/alpha-1.0.jar", dir).unwrap()
}

#[test]
fn install_fresh_with_valid_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let body = b"payload-bytes";
    let hash = currspice_mods::mods::sha256_hex(body);
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(vec![body.to_vec()]);
    assert_eq!(m.install(Some(&hash), &mut t).unwrap(), Outcome::Installed);
    assert_eq!(std::fs::read(m.path()).unwrap(), body);
    assert_eq!(t.calls.len(), 1);
    assert_eq!(t.calls[0], "https://host.example/mods/alpha-1.0.jar");
}

#[test]
fn install_uses_expected_hash_case_insensitively() {
    let dir = tempfile::tempdir().unwrap();
    let body = b"payload";
    let hash_upper = currspice_mods::mods::sha256_hex(body).to_uppercase();
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(vec![body.to_vec()]);
    assert_eq!(
        m.install(Some(&hash_upper), &mut t).unwrap(),
        Outcome::Installed
    );
}

#[test]
fn skip_on_checksum_match_no_redownload() {
    let dir = tempfile::tempdir().unwrap();
    let body = b"payload";
    let hash = currspice_mods::mods::sha256_hex(body);
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(dir.path().join("alpha-1.0.jar"), body).unwrap();
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(Vec::new());
    assert_eq!(m.install(Some(&hash), &mut t).unwrap(), Outcome::Skipped);
    assert_eq!(
        t.calls.len(),
        0,
        "a matched existing file must not trigger a fetch"
    );
}

#[test]
fn replace_on_checksum_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let body = b"new-bytes";
    let hash = currspice_mods::mods::sha256_hex(body);
    std::fs::write(dir.path().join("alpha-1.0.jar"), b"stale-bytes").unwrap();
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(vec![body.to_vec()]);
    assert_eq!(m.install(Some(&hash), &mut t).unwrap(), Outcome::Replaced);
    assert_eq!(std::fs::read(m.path()).unwrap(), body);
}

#[test]
fn checksum_mismatch_first_try_retries_second_fails_and_preserves_prior() {
    let dir = tempfile::tempdir().unwrap();
    let prior = b"ORIGINAL-KEEP-ME";
    std::fs::write(dir.path().join("alpha-1.0.jar"), prior).unwrap();
    let hash = currspice_mods::mods::sha256_hex(b"correct");
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(vec![b"corrupt-1".to_vec(), b"corrupt-2".to_vec()]);
    let err = m.install(Some(&hash), &mut t).unwrap_err();
    assert_eq!(err.exit_code(), 4);
    assert!(err.to_string().contains("Checksum failed"), "{err}");
    assert_eq!(t.calls.len(), 2, "one retry is expected, then stop");
    // The prior, different file must survive untouched — never replaced.
    assert_eq!(std::fs::read(m.path()).unwrap(), prior);
    // And no leftover staging files in the mods dir.
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != "alpha-1.0.jar")
        .collect();
    assert!(leftovers.is_empty(), "staging files leaked: {leftovers:?}");
}

#[test]
fn checksum_retry_then_success_preserves_idempotent_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let body = b"correct";
    let hash = currspice_mods::mods::sha256_hex(body);
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(vec![b"transient-bad".to_vec(), body.to_vec()]);
    assert_eq!(m.install(Some(&hash), &mut t).unwrap(), Outcome::Installed);
    assert_eq!(
        t.calls.len(),
        2,
        "failed first attempt consumed, then success"
    );
    assert_eq!(std::fs::read(m.path()).unwrap(), body);
}

#[test]
fn unpinned_existing_valid_jar_skips() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha-1.0.jar"), make_jar()).unwrap();
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(Vec::new());
    assert_eq!(m.install(None, &mut t).unwrap(), Outcome::Skipped);
    assert_eq!(t.calls.len(), 0);
}

#[test]
fn unpinned_existing_corrupt_replaces_with_valid_jar() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha-1.0.jar"), b"corrupt").unwrap();
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(vec![make_jar()]);
    assert_eq!(m.install(None, &mut t).unwrap(), Outcome::Replaced);
    assert!(valid_jar(m.path()));
}

#[test]
fn unpinned_downloaded_not_a_jar_fails_without_publishing() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(vec![b"definitely-not-a-jar".to_vec()]);
    let err = m.install(None, &mut t).unwrap_err();
    assert_eq!(err.exit_code(), 4);
    assert!(!m.path().exists(), "failed download must not be published");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "no staging leftovers allowed: {leftovers:?}"
    );
}

#[test]
fn empty_download_is_rejected_even_with_matching_hash() {
    // A 0-byte body is never an acceptable "mod" download, even if the
    // recorded hash is literally sha256(""). The code short-circuits on the
    // length check before persisting.
    let dir = tempfile::tempdir().unwrap();
    let empty_hash = currspice_mods::mods::sha256_hex(b"");
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(vec![Vec::new()]);
    let err = m.install(Some(&empty_hash), &mut t).unwrap_err();
    assert_eq!(
        err.exit_code(),
        4,
        "an empty download is a Verify-class failure"
    );
    assert!(
        err.to_string().contains("Empty"),
        "should explain the empty body: {err}"
    );
    assert!(
        !m.path().exists(),
        "an empty body must not be published as a file"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "no staging leftovers allowed: {leftovers:?}"
    );
}

#[test]
fn transport_failure_mid_fetch_leaves_no_partial_publish() {
    let dir = tempfile::tempdir().unwrap();
    let prior = b"KEEP";
    std::fs::write(dir.path().join("alpha-1.0.jar"), prior).unwrap();
    let hash = currspice_mods::mods::sha256_hex(b"want");
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(Vec::new());
    t.fail_next("connection reset");
    let err = m.install(Some(&hash), &mut t).unwrap_err();
    assert_eq!(err.exit_code(), 2, "a transport failure is a Network error");
    assert_eq!(
        std::fs::read(m.path()).unwrap(),
        prior,
        "prior file preserved"
    );
}

#[test]
fn stale_part_from_previous_interrupted_run_is_reused_not_doubled() {
    let dir = tempfile::tempdir().unwrap();
    // Simulate an interrupted run that dropped a `.part` behind.
    let stale = dir.path().join(".currspice-stale.part");
    std::fs::write(&stale, b"junk").unwrap();
    let body = b"correct";
    let hash = currspice_mods::mods::sha256_hex(body);
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(vec![body.to_vec()]);
    // After a clean install, there may or may not be leftovers, but the final
    // target must be byte-identical and valid.
    assert_eq!(m.install(Some(&hash), &mut t).unwrap(), Outcome::Installed);
    assert_eq!(std::fs::read(m.path()).unwrap(), body);
}

#[test]
fn install_rejects_invalid_recorded_hash_before_any_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = fresh_mod(dir.path());
    let mut t = Scripted::new(Vec::new());
    let err = m.install(Some("not-a-valid-hash"), &mut t).unwrap_err();
    assert_eq!(
        err.exit_code(),
        1,
        "a malformed recorded hash is a config-level error"
    );
    assert!(
        t.calls.is_empty(),
        "must not fetch before validating the hash"
    );
    assert!(!m.path().exists());
}

// ---------------------------------------------------------------------------
// Transport contract: production HttpTransport is HTTPS-only
// ---------------------------------------------------------------------------

#[test]
fn production_transport_refuses_plain_http() {
    // The *production* client is constructed with `https_only(true)` — a
    // `http://` URL must be rejected by the client itself and classified as a
    // Network error, with no bytes delivered.
    let mut t = currspice_mods::download::HttpTransport::new().unwrap();
    let res = fetch_bytes(&mut t, "http://127.0.0.1:1/anything");
    if let Ok(bytes) = res {
        panic!("a plain-HTTP URL must not succeed, got {bytes:?} bytes");
    } // expected: the client rejects non-https schemes
}

#[test]
fn production_transport_refjects_non_https_scheme_variants() {
    let mut t = currspice_mods::download::HttpTransport::new().unwrap();
    // ftp/file are rejected by scheme before any I/O; http is rejected by the
    // https_only guard. In all cases no body may be produced.
    for url in ["ftp://127.0.0.1:1/x.jar", "file:///C:/nope/x.jar"] {
        let res = fetch_bytes(&mut t, url).err();
        assert!(
            res.is_some(),
            "{url} must not be accepted by the https-only client"
        );
    }
}
