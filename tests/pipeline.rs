use currspice_mods::{
    config::Config,
    download::{InstallError, Transport},
    mods::{Fabric, sha256_hex},
    pipeline::{Options, Runtime, install},
};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Default)]
struct Fake {
    calls: usize,
    mod_bodies: Vec<Vec<u8>>,
}
impl Transport for Fake {
    fn fetch(&mut self, url: &str, out: &mut dyn Write) -> Result<(), InstallError> {
        self.calls += 1;
        if url.ends_with("f.jar") {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            zip.start_file(
                "META-INF/MANIFEST.MF",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(b"Manifest-Version: 1.0\n").unwrap();
            out.write_all(&zip.finish().unwrap().into_inner()).unwrap();
        } else {
            out.write_all(&self.mod_bodies.remove(0)).unwrap();
        }
        Ok(())
    }
}
#[derive(Default)]
struct Runner {
    java_calls: usize,
    runs: usize,
    fail_profile: bool,
}
impl Runtime for Runner {
    fn java(&mut self) -> Result<PathBuf, InstallError> {
        self.java_calls += 1;
        Ok("fake-java".into())
    }
    fn fabric(&mut self, f: &Fabric, _: &Path, _: bool) -> Result<(), InstallError> {
        self.runs += 1;
        if !self.fail_profile {
            std::fs::write(f.mc_dir().join("launcher_profiles.json"), r#"{"profiles":{"fabric-loader-1.21.11":{"lastVersionId":"fabric-loader-0.18.4-1.21.11"}}}"#).unwrap();
        }
        Ok(())
    }
}
fn config() -> Config {
    serde_yaml::from_str(&format!("mc_version: 1.21.11\ninstaller_version: 0.18.4\nfabric: https://host/f.jar\nserver_mods:\n- url: https://host/mod.jar\n  sha256: {}\n", sha256_hex(b"good"))).unwrap()
}
#[test]
fn full_pipeline_install_rerun_corruption_retry_and_failure() {
    let dir = tempfile::tempdir().unwrap();
    let opts = Options {
        mc_dir: dir.path().to_path_buf(),
        ..Options::default()
    };
    let mut http = Fake {
        mod_bodies: vec![b"good".to_vec()],
        ..Fake::default()
    };
    let mut runner = Runner::default();
    assert!(
        install(config(), &opts, &mut http, &mut runner)
            .unwrap()
            .contains("installed: 1")
    );
    assert_eq!(
        std::fs::read(dir.path().join("mods/mod.jar")).unwrap(),
        b"good"
    );
    assert!(
        install(config(), &opts, &mut http, &mut runner)
            .unwrap()
            .contains("skipped: 1")
    );
    std::fs::write(dir.path().join("mods/mod.jar"), b"prior").unwrap();
    http.mod_bodies = vec![b"bad".to_vec(), b"good".to_vec()];
    assert!(
        install(config(), &opts, &mut http, &mut runner)
            .unwrap()
            .contains("replaced: 1")
    );
    std::fs::write(dir.path().join("mods/mod.jar"), b"prior").unwrap();
    http.mod_bodies = vec![b"bad".to_vec(), b"bad".to_vec()];
    assert_eq!(
        install(config(), &opts, &mut http, &mut runner)
            .unwrap_err()
            .exit_code(),
        4
    );
    assert_eq!(
        std::fs::read(dir.path().join("mods/mod.jar")).unwrap(),
        b"prior"
    );
    assert_eq!(
        std::fs::read_dir(dir.path().join("mods")).unwrap().count(),
        1
    );
}
#[test]
fn dry_run_and_offline_failure_are_side_effect_free() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = Options {
        mc_dir: dir.path().to_path_buf(),
        dry_run: true,
        ..Options::default()
    };
    let mut http = Fake::default();
    let mut runner = Runner::default();
    assert!(
        install(config(), &opts, &mut http, &mut runner)
            .unwrap()
            .contains("DRY RUN")
    );
    opts.dry_run = false;
    opts.no_download = true;
    assert!(install(config(), &opts, &mut http, &mut runner).is_err());
    assert_eq!((http.calls, runner.java_calls, runner.runs), (0, 0, 0));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[test]
fn missing_fabric_profile_stops_before_mods() {
    let dir = tempfile::tempdir().unwrap();
    let opts = Options {
        mc_dir: dir.path().to_path_buf(),
        ..Options::default()
    };
    let mut http = Fake::default();
    let mut runner = Runner {
        fail_profile: true,
        ..Runner::default()
    };
    assert_eq!(
        install(config(), &opts, &mut http, &mut runner)
            .unwrap_err()
            .exit_code(),
        4
    );
    assert!(!dir.path().join("mods").exists());
}
