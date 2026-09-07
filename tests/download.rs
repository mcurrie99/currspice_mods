use currspice_mods::download::{InstallError, Outcome, Transport};
use currspice_mods::mods::{Mod, sha256_hex};
use std::io::Write;
struct Stub {
    bodies: Vec<Vec<u8>>,
    calls: usize,
}
impl Transport for Stub {
    fn fetch(&mut self, _: &str, out: &mut dyn Write) -> Result<(), InstallError> {
        self.calls += 1;
        out.write_all(&self.bodies.remove(0)).unwrap();
        Ok(())
    }
}
fn jar() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file(
        "META-INF/MANIFEST.MF",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"Manifest-Version: 1.0\n").unwrap();
    zip.finish().unwrap().into_inner()
}
#[test]
fn unpinned_corrupt_cache_is_not_a_successful_install() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = Mod::new("https://host/f.jar", dir.path()).unwrap();
    std::fs::write(m.path(), b"corrupt cache").unwrap();
    let mut http = Stub {
        bodies: vec![jar()],
        calls: 0,
    };
    assert_eq!(m.install(None, &mut http).unwrap(), Outcome::Replaced);
    assert_eq!(http.calls, 1);
    std::fs::remove_file(m.path()).unwrap();
    http.bodies = vec![b"not a jar".to_vec()];
    assert_eq!(m.install(None, &mut http).unwrap_err().exit_code(), 4);
    assert!(!m.path().exists());
}
#[test]
fn checksum_retry_replace_skip_and_failure_preserve_prior_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = Mod::new("https://host/mod.jar", dir.path()).unwrap();
    let hash = sha256_hex(b"good");
    let mut http = Stub {
        bodies: vec![b"bad".to_vec(), b"good".to_vec()],
        calls: 0,
    };
    assert_eq!(
        m.install(Some(&hash), &mut http).unwrap(),
        Outcome::Installed
    );
    assert_eq!(http.calls, 2);
    assert_eq!(m.install(Some(&hash), &mut http).unwrap(), Outcome::Skipped);
    std::fs::write(m.path(), b"prior").unwrap();
    http.bodies = vec![b"bad".to_vec(), b"bad".to_vec()];
    let err = m.install(Some(&hash), &mut http).unwrap_err();
    assert_eq!(err.exit_code(), 4);
    assert!(err.to_string().contains(&hash));
    assert_eq!(std::fs::read(m.path()).unwrap(), b"prior");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    http.bodies = vec![b"good".to_vec()];
    assert_eq!(
        m.install(Some(&hash), &mut http).unwrap(),
        Outcome::Replaced
    );
}
