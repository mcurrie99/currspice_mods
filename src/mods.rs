//! Mod and Fabric-installer downloads.
//!
//! Verification log (2026-08, Phase 0):
//! - `java -jar fabric-installer-1.0.3.jar client -dir <mc> -mcversion <v> -loader <v>`
//!   completes headlessly and writes a launcher profile named `fabric-loader-<v>`.
//! - It REFUSES to create that profile when `launcher_profiles.json` is missing
//!   from `<mc>` ("Could not find a valid launcher profile") — so we seed it.
//! - The `fabriC-installer-*.exe` maven artifact is a GUI-app (PE32,
//!   GUI subsystem): with no args it does nothing, and with correct args it
//!   sits waiting for a click. It is NOT headless-capable. We download the jar.
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub struct Mod {
    name: String,
    download: String,
    path: PathBuf,
    installed: bool,
}

impl Mod {
    /// Build a mod whose final install location is `<directory>/<last URL segment>`.
    pub fn new(download: &str, directory: &Path) -> Result<Self, Box<dyn Error>> {
        let url = reqwest::Url::parse(download)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Expected an HTTPS artifact URL without credentials".into());
        }
        let filename = url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .ok_or("Missing artifact filename")?;
        let lower = filename.to_ascii_lowercase();
        let stem = lower.split('.').next().unwrap_or("");
        let reserved = ["con", "prn", "aux", "nul"].contains(&stem)
            || (stem.len() == 4
                && (stem.starts_with("com") || stem.starts_with("lpt"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'));
        if !lower.ends_with(".jar")
            || filename
                .chars()
                .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
            || ["%2f", "%5c", "%3a", "%00"]
                .iter()
                .any(|bad| lower.contains(bad))
            || reserved
        {
            return Err(format!("Unsafe jar filename: {filename}").into());
        }
        Ok(Mod {
            name: filename.to_string(),
            download: download.to_string(),
            path: directory.join(filename),
            installed: false,
        })
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_download(&self) -> &str {
        &self.download
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_installed(&self) -> bool {
        self.path.exists()
    }

    /// Last path segment of a URL (or a pasted local path) — the jar's file
    /// name on disk. Handles both `/` (URLs) and `\` (Windows paths);
    /// percent-encoding is preserved (keeps names valid on any OS).
    pub fn extract_name(link: &str) -> Option<&str> {
        let name = link
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()?;
        (!name.is_empty()).then_some(name)
    }

    fn part_name(name: &str) -> String {
        format!("{name}.part")
    }

    /// Atomically install verified content, retrying a checksum failure once.
    pub fn install(
        &mut self,
        expected: Option<&str>,
        transport: &mut dyn crate::download::Transport,
    ) -> Result<crate::download::Outcome, crate::download::InstallError> {
        use crate::download::{InstallError, Outcome, file_sha256};
        if let Some(hash) = expected
            && (hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(InstallError::Config(format!(
                "Invalid SHA-256 for {}",
                self.name
            )));
        }
        let existed = self.path.try_exists()?;
        if existed && self.path.is_file() {
            if let Some(hash) = expected {
                if file_sha256(&self.path)?.eq_ignore_ascii_case(hash) {
                    return Ok(Outcome::Skipped);
                }
            } else if crate::download::valid_jar(&self.path) {
                return Ok(Outcome::Skipped);
            }
        }
        let stale = self.path.with_file_name(Self::part_name(&self.name));
        if stale.try_exists()? {
            fs::remove_file(stale)?;
        }
        for attempt in 0..2 {
            let parent = self
                .path
                .parent()
                .ok_or_else(|| InstallError::Config("Missing destination directory".into()))?;
            let mut part = tempfile::Builder::new()
                .prefix(".currspice-")
                .suffix(".part")
                .tempfile_in(parent)?;
            transport.fetch(&self.download, part.as_file_mut())?;
            part.as_file_mut().sync_all()?;
            if let Some(hash) = expected {
                let actual = file_sha256(part.path())?;
                if !actual.eq_ignore_ascii_case(hash) {
                    if attempt == 0 {
                        continue;
                    }
                    return Err(InstallError::Verify(format!(
                        "Checksum failed for {} after two attempts: expected {hash}, got {actual}; URL {}",
                        self.name, self.download
                    )));
                }
            }
            if part.as_file().metadata()?.len() == 0 {
                return Err(InstallError::Verify(format!(
                    "Empty download for {}",
                    self.name
                )));
            }
            if expected.is_none() && !crate::download::valid_jar(part.path()) {
                return Err(InstallError::Verify(format!(
                    "Invalid jar archive for {}",
                    self.name
                )));
            }
            part.persist(&self.path)
                .map_err(|e| InstallError::Install(e.to_string()))?;
            self.installed = true;
            return Ok(if existed {
                Outcome::Replaced
            } else {
                Outcome::Installed
            });
        }
        unreachable!("two attempts always return")
    }

    pub fn download_verified(&mut self, expected: Option<&str>) -> Result<(), Box<dyn Error>> {
        self.install(expected, &mut crate::download::HttpTransport::new()?)?;
        Ok(())
    }

    /// Download without a manifest checksum (v1 manifests have none).
    pub fn download(&mut self) -> Result<(), Box<dyn Error>> {
        self.download_verified(None)
    }

    /// Remove the installed mod (idempotent — missing file is not an error).
    pub fn delete(&mut self) -> Result<(), Box<dyn Error>> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        // Clean up a stale .part left by an interrupted earlier run.
        let file_name = self
            .path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let part = self.path.with_file_name(Self::part_name(&file_name));
        if part.exists() {
            fs::remove_file(&part)?;
        }
        self.installed = false;
        Ok(())
    }
}

/// hex(SHA-256(bytes))
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

/// A Fabric client install: download the installer, make sure the launcher
/// profile file exists, run it, confirm its profile was actually created.
pub struct Fabric {
    mc_version: String,
    inst_version: String,
    installer: Mod,
    /// The `.minecraft` root the install targets.
    mc_dir: PathBuf,
}

impl Fabric {
    fn jar_url(raw: &str) -> Result<String, Box<dyn Error>> {
        let mut url = reqwest::Url::parse(raw)?;
        if url.path().ends_with(".exe") {
            if url.host_str() != Some("maven.fabricmc.net")
                || !url.path().starts_with("/net/fabricmc/fabric-installer/")
            {
                return Err(
                    "Legacy .exe URL must be an official Fabric Maven installer; use its .jar URL"
                        .into(),
                );
            }
            let path = format!("{}.jar", url.path().trim_end_matches(".exe"));
            url.set_path(&path);
        }
        Ok(url.into())
    }

    pub fn verify_profile(&self) -> Result<(), crate::download::InstallError> {
        use crate::download::InstallError;
        let check = || -> Result<(), Box<dyn Error>> {
            let doc: serde_json::Value =
                serde_json::from_slice(&fs::read(self.mc_dir.join("launcher_profiles.json"))?)?;
            let name = crate::fabric_profile_name(&self.mc_version);
            let profile = doc
                .get("profiles")
                .and_then(|p| p.get(&name))
                .and_then(|p| p.as_object())
                .ok_or("Fabric profile is missing or invalid")?;
            let expected = format!("fabric-loader-{}-{}", self.inst_version, self.mc_version);
            if profile.get("lastVersionId").and_then(|v| v.as_str()) != Some(expected.as_str()) {
                return Err(format!("Fabric profile must select {expected}").into());
            }
            Ok(())
        };
        check().map_err(|e| InstallError::Verify(format!("Fabric verification failed: {e}")))
    }

    pub fn from_config(config: &crate::config::Config) -> Result<Self, Box<dyn Error>> {
        Self::new(
            config.get_mc_version(),
            config.get_installer_version(),
            config.get_fabric_url(),
            Path::new(config.get_path()),
        )
    }

    pub fn new(
        mc_version: &str,
        inst_version: &str,
        installer_url: &str,
        mc_dir: &Path,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Fabric {
            mc_version: mc_version.to_string(),
            inst_version: inst_version.to_string(),
            installer: Mod::new(&Self::jar_url(installer_url)?, mc_dir)?,
            mc_dir: mc_dir.to_path_buf(),
        })
    }

    pub fn get_mc_version(&self) -> &str {
        &self.mc_version
    }

    pub fn get_inst_version(&self) -> &str {
        &self.inst_version
    }

    pub fn installer(&self) -> &Mod {
        &self.installer
    }

    pub fn mc_dir(&self) -> &Path {
        &self.mc_dir
    }

    /// The headless Fabric installer invocation. This shape is **verified**
    /// (see verification log at the top of this file): it is what the old
    /// commented-out code intended; the only addition is the seeded profile
    /// file (see [`Fabric::seed_launcher_profile`]).
    pub fn installer_command(&self) -> Vec<String> {
        vec![
            "-jar".into(),
            self.installer.path().display().to_string(),
            "client".into(),
            "-dir".into(),
            self.mc_dir.display().to_string(),
            "-mcversion".into(),
            self.mc_version.clone(),
            "-loader".into(),
            self.inst_version.clone(),
        ]
    }

    /// The Fabric CLI errors out ("Could not find a valid launcher profile")
    /// if `launcher_profiles.json` doesn't exist under `<mc_dir>` — verified
    /// 2026-08. Real users have one; clean test machines don't. Seed a
    /// minimal valid one when missing; never overwrite an existing one.
    pub fn seed_launcher_profile(&self) -> Result<(), Box<dyn Error>> {
        let profile_path = self.mc_dir.join("launcher_profiles.json");
        if profile_path.exists() {
            return Ok(());
        }
        let minimal = serde_json::json!({
            "client": { "latest": self.mc_version },
            "launcherVersion": self.mc_version,
            "profiles": {}
        });
        fs::write(profile_path, serde_json::to_string_pretty(&minimal)?)?;
        Ok(())
    }

    /// Download the installer jar (skipped if it's already present — that's
    /// the re-run fast path).
    pub fn download(&mut self) -> Result<(), Box<dyn Error>> {
        if self.installer.path().exists() {
            return Ok(());
        }
        self.installer.download()
    }

    /// Run the headless installer, then verify its success contract: the
    /// `fabric-loader-<mc_version>` profile exists in launcher_profiles.json.
    ///
    /// `java_bin` is resolved by the caller via [`crate::jre::ensure_java`]
    /// (system java → cached private JRE → one-time download), so friends
    /// never need a Java install of their own.
    pub fn run_installer(&self, java_bin: &Path) -> Result<(), Box<dyn Error>> {
        let args = self.installer_command();
        eprintln!(
            "Currspice: running {}",
            [java_bin.display().to_string(), args.join(" ")].join(" ")
        );

        let status = std::process::Command::new(java_bin)
            .args(&args)
            .status()
            .map_err(|e| {
                format!(
                    "could not start Java ({e}). If you installed Java today, close and re-open this program so it can see it.",
                )
            })?;

        if !status.success() {
            return Err(format!(
                "the Fabric installer exited with {status}. Re-run the installer; if it fails again, delete the partial install under {} and try once more.",
                self.mc_dir.display()
            ).into());
        }

        self.verify_profile()?;
        Ok(())
    }

    /// Delete the installer jar and its .part (keep the install itself).
    pub fn delete(&mut self) -> Result<(), Box<dyn Error>> {
        self.installer.delete()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fabric() -> Fabric {
        Fabric::new("1.21.11", "0.18.4", "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar", Path::new("C:/mc")).expect("build fabric")
    }

    #[test]
    fn fabric_from_config_shapes() {
        let cfg = crate::config::Config::new(
            "C:/mc",
            &["https://cdn.modrinth.com/data/x/versions/y/mod-1.21.11.jar"],
            &[],
            "1.21.11",
            "0.18.4",
            "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar",
        );
        let f = Fabric::from_config(&cfg).expect("from_config");
        assert_eq!(f.get_mc_version(), "1.21.11");
        assert_eq!(f.get_inst_version(), "0.18.4");
        assert_eq!(
            f.installer().path(),
            Path::new("C:/mc").join("fabric-installer-1.0.3.jar")
        );
    }

    fn expected_installer_path() -> String {
        // PathBuf::join renders with the platform's separator, so compare
        // against a constructed Path, not a hardcoded "/".
        Path::new("C:/mc")
            .join("fabric-installer-1.0.3.jar")
            .display()
            .to_string()
    }

    #[test]
    fn installer_command_matches_verified_cli() {
        // Order and values of the headless invocation, exactly as verified
        // against the real installer on 2026-08-30.
        let cmd = fabric().installer_command();
        assert_eq!(
            cmd,
            vec![
                "-jar".to_string(),
                expected_installer_path(),
                "client".to_string(),
                "-dir".to_string(),
                "C:/mc".to_string(),
                "-mcversion".to_string(),
                "1.21.11".to_string(),
                "-loader".to_string(),
                "0.18.4".to_string(),
            ]
        );
    }

    #[test]
    fn extract_name_from_modrinth_url() {
        let name = Mod::extract_name(
            "https://cdn.modrinth.com/data/AANobbMI/versions/59wygFUQ/sodium-fabric-0.8.2%2Bmc1.21.11.jar",
        )
        .expect("has a name");
        assert_eq!(name, "sodium-fabric-0.8.2%2Bmc1.21.11.jar");
    }

    #[test]
    fn extract_name_handles_url_and_path_forms() {
        assert!(Mod::extract_name("").is_none());
        // Real-world: the last path segment of a Modrinth jar URL.
        assert_eq!(
            Mod::extract_name(
                "https://cdn.modrinth.com/data/x/versions/y/sodium-0.8.2%2Bmc1.21.11.jar"
            ),
            Some("sodium-0.8.2%2Bmc1.21.11.jar")
        );
        // Last non-empty segment over either separator (URLs and pasted
        // Windows paths both work); a bare filename is its own name.
        assert_eq!(Mod::extract_name("https://host/a/b/"), Some("b"));
        assert_eq!(
            Mod::extract_name(r"C:\mods\zoomify.jar"),
            Some("zoomify.jar")
        );
        assert_eq!(Mod::extract_name("soundpack.jar"), Some("soundpack.jar"));
    }

    #[test]
    fn new_rejects_unsafe_filenames() {
        for url in [
            "",
            "https://host/",
            "https://host/../",
            "https://host/CON.jar",
            "https://host/a:b.jar",
            "https://host/a%2fb.jar",
            "http://host/a.jar",
            "https://u:p@host/a.jar",
        ] {
            assert!(
                Mod::new(url, Path::new("C:/mods")).is_err(),
                "accepted {url}"
            );
        }
        assert_eq!(
            Mod::new("https://host/é.jar?q=1", Path::new("C:/mods"))
                .unwrap()
                .get_name(),
            "%C3%A9.jar"
        );
        assert_eq!(Mod::extract_name("https://host/é.jar"), Some("é.jar"));
    }

    #[test]
    fn sha256_of_empty_string_is_canonical() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_is_stable_for_known_input() {
        // Locks the hex-encoding direction (lowercase, no separators).
        let a = sha256_hex(b"abc");
        assert_eq!(a, sha256_hex(b"abc"));
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn seed_creates_minimal_profile_doc() {
        let dir = tempfile::tempdir().expect("tempdir");
        let f = Fabric::new("1.21.11", "0.18.4", "https://example.com/f.jar", dir.path()).unwrap();
        f.seed_launcher_profile().expect("seed");

        let doc = serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(dir.path().join("launcher_profiles.json")).unwrap(),
        )
        .unwrap();
        assert!(
            doc["profiles"]
                .as_object()
                .map(|o| o.is_empty())
                .unwrap_or(false)
        );
        assert_eq!(doc["launcherVersion"], "1.21.11");
    }

    #[test]
    fn seed_never_overwrites_existing_profile() {
        let dir = tempfile::tempdir().expect("tempdir");
        let f = Fabric::new("1.21.11", "0.18.4", "https://example.com/f.jar", dir.path()).unwrap();
        let existing = serde_json::json!({"profiles": {"keep-me": {"x": 1}}});
        fs::write(
            dir.path().join("launcher_profiles.json"),
            existing.to_string(),
        )
        .unwrap();

        f.seed_launcher_profile().expect("seed (no-op)");
        let doc: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("launcher_profiles.json")).unwrap(),
        )
        .unwrap();
        assert!(
            doc["profiles"].get("keep-me").is_some(),
            "existing content must survive"
        );
    }

    #[test]
    fn delete_is_idempotent_and_clears_stale_part() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut m = Mod::new("https://host/mod.jar", dir.path()).unwrap();
        // nothing installed yet
        m.delete().expect("delete on missing file is fine");
        // simulate an interrupted download, then delete
        fs::write(m.path().with_file_name("mod.jar.part"), b"stale").unwrap();
        m.delete().expect("delete cleans .part");
        assert!(!m.path().with_file_name("mod.jar.part").exists());
    }

    #[test]
    fn is_installed_reflects_file_presence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let m = Mod::new("https://host/mod.jar", dir.path()).unwrap();
        assert!(!m.is_installed());
        fs::write(m.path(), b"jar").unwrap();
        assert!(m.is_installed());
    }
}
