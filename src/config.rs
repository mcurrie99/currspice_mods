//! Configuration: the `mods.yaml` manifest (load / download / verify) and
//! the Minecraft launcher-profile edit.
//!
//! The manifest is the single source of truth for what gets installed
//! (ROADMAP.md P1) — all mod knowledge lives here, never in code.
use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{JVM_ARGS, LOGO, PROFILE_NAME, fabric_profile_name};

/// One mod entry in the manifest. Accepts both shapes, so existing v1 files
/// keep working untouched:
///
/// ```yaml
/// server_mods:
/// - https://cdn.modrinth.com/.../sodium-....jar          # v1: URL only
/// - url: https://cdn.modrinth.com/.../iris-....jar
///   sha256: "8f2c…"                                          # optional, enforced at install
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub enum ModRef {
    /// v1 form — the CDN link alone; no integrity check possible at install.
    Url(String),
    /// v1.5 form — the link plus an optional recorded sha256 (64 hex chars).
    /// The installer enforces it and reports both digests on a mismatch, so a
    /// changed CDN jar is never installed silently as a different file.
    Entry { url: String, sha256: Option<String> },
}

impl ModRef {
    pub fn url(&self) -> &str {
        match self {
            ModRef::Url(u) | ModRef::Entry { url: u, .. } => u,
        }
    }

    /// Recorded checksum, if any (never ""-or-whitespace).
    pub fn sha256(&self) -> Option<&str> {
        match self {
            ModRef::Url(_) => None,
            ModRef::Entry { sha256, .. } => sha256.as_deref(),
        }
    }

    /// A recorded sha256 must be a valid 64-char hex digest — a bad one means
    /// a hand-edit went off the rails, and failing at load with a named mod is
    /// much better than failing mid-install with two wrong hashes in the log.
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        if let Some(h) = self.sha256() {
            let ok = h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit());
            if !ok {
                return Err(format!(
                    "config: the sha256 for {} is not a 64-character hex digest ({} chars — expected a 64-char lowercase hex string)",
                    self.url(),
                    h.chars().count()
                )
                .into());
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Config {
    /// Path to the player's Minecraft installation. Filled in at runtime by
    /// the installer; the manifest itself doesn't pin it (see the
    /// `empty_path_defaults` test documenting the serde default).
    #[serde(default)]
    path: String,
    /// Minecraft version the whole manifest targets (e.g. 1.21.11).
    mc_version: String,
    /// Fabric loader version to install with.
    installer_version: String,
    /// Mods every friend must have for the server.
    server_mods: Vec<ModRef>,
    /// Per-user mods (opt-in for friends).
    #[serde(default)]
    user_mods: Vec<ModRef>,
    /// Fabric installer artifact to download (jar or exe — see
    /// `Fabric::run_installer` for why the jar is what we execute).
    fabric: String,
}

impl Config {
    /// Explicit > existing CWD manifest > compiled canonical default. Never fetches.
    pub fn select(explicit: Option<&Path>, cwd: &Path) -> Result<Self, Box<dyn Error>> {
        if let Some(path) = explicit {
            return Self::load_new(path);
        }
        let local = cwd.join("mods.yaml");
        match fs::symlink_metadata(&local) {
            Ok(_) => Self::load_new(&local),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let config: Self = serde_yaml::from_str(include_str!("../docs/mods.yaml"))?;
                config.validate()?;
                Ok(config)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// In-memory construction (tests, defaults, and future generation from
    /// the dashboard).
    pub fn new(
        path: &str,
        server_mods: &[&str],
        user_mods: &[&str],
        mc_version: &str,
        installer_version: &str,
        fabric_url: &str,
    ) -> Self {
        Config {
            path: String::from(path),
            server_mods: server_mods
                .iter()
                .map(|s| ModRef::Url(s.to_string()))
                .collect(),
            user_mods: user_mods
                .iter()
                .map(|s| ModRef::Url(s.to_string()))
                .collect(),
            mc_version: String::from(mc_version),
            installer_version: String::from(installer_version),
            fabric: String::from(fabric_url),
        }
    }

    /// Load and parse a manifest; validates basic shape before anything
    /// network-related happens.
    pub fn load_new(filepath: &Path) -> Result<Config, Box<dyn Error>> {
        let file = File::open(filepath)?;
        let config: Config = serde_yaml::from_reader(file)?;
        config.validate()?;
        Ok(config)
    }

    /// Basic integrity checks — cheap, offline, and they catch the failure
    /// modes that would otherwise surface as a confusing mid-install error.
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        if self.mc_version.trim().is_empty() {
            return Err(
                "config: mc_version is missing — the installer can't pick a Minecraft version"
                    .into(),
            );
        }
        if self.installer_version.trim().is_empty() {
            return Err("config: installer_version is missing — the installer can't pick a Fabric loader version".into());
        }
        if !self.fabric.starts_with("https://") {
            return Err(
                "config: the fabric installer URL is missing or not an https:// URL".into(),
            );
        }
        for item in self.mod_refs() {
            item.validate()?;
        }
        if self.server_mods.is_empty() {
            return Err(
                "config: no mods are listed under server_mods — nothing would be installed".into(),
            );
        }
        for link in self.all_mods() {
            if !link.starts_with("https://") {
                return Err(format!(
                    "config: mod link '{link}' is not an https:// URL (a typo turns into a 404 mid-install)"
                )
                .into());
            }
        }
        Ok(())
    }

    /// Phase-0 drift guard (ROADMAP.md): the manifest pins one MC version and
    /// no mod may target a *different version of the same MC series*. This is
    /// the exact mix that landed in a pre-release branch (mc_version 1.21.1
    /// with 1.21.11 build URLs) and would install a mod set the server
    /// can't load.
    ///
    /// Deliberate scope: a URL that names no same-series version at all
    /// (e.g. the Fabric Kotlin runtime jar, which isn't MC-versioned) passes
    /// — we can't disprove it, and the real hazard is two *different* pinned
    /// series versions coexisting in one install.
    pub fn verify_version_consistency(&self) -> Result<(), Box<dyn Error>> {
        let mut offenders = Vec::new();
        for link in self.all_mods() {
            if conflict_in(link, &self.mc_version) {
                offenders.push(link.clone());
            }
        }
        if offenders.is_empty() {
            return Ok(());
        }
        let list = offenders.join("\n    - ");
        Err(format!(
            "config: the manifest declares Minecraft {v}, but these mods pin a different version of the same series:\n    - {list}\nA friend installing them gets a version mismatch the server can't load. Fix the manifest (docs/mods.yaml is the source of truth) — pick one version, everywhere.",
            v = self.mc_version,
        )
        .into())
    }

    /// Download a remote manifest to `filepath`, then load (and validate)
    /// it. Parent dir is created if needed.
    pub fn download_new(filepath: &Path, link: &str) -> Result<Config, Box<dyn Error>> {
        let response = reqwest::blocking::get(link)?;
        let content = response.bytes()?;
        if let Some(parent) = filepath.parent()
            && !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        fs::write(filepath, &content)?;
        Self::load_new(filepath)
    }

    // ---- accessors ---------------------------------------------------------

    pub fn get_path(&self) -> &str {
        &self.path
    }

    pub fn get_mc_version(&self) -> &str {
        &self.mc_version
    }

    pub fn get_installer_version(&self) -> &str {
        &self.installer_version
    }

    pub fn get_fabric_url(&self) -> &str {
        &self.fabric
    }

    pub fn get_server_mods(&self) -> &[ModRef] {
        &self.server_mods
    }

    pub fn get_user_mods(&self) -> &[ModRef] {
        &self.user_mods
    }

    /// Every mod that must be installed: the required server mods first,
    /// then user mods. Single loop in main.rs (old code only did the first
    /// half — that's how `user_mods` went to rot).
    pub fn all_mods(&self) -> impl Iterator<Item = &String> {
        self.server_mods
            .iter()
            .chain(self.user_mods.iter())
            .map(|m| match m {
                ModRef::Url(u) | ModRef::Entry { url: u, .. } => u,
            })
    }

    pub fn mod_refs(&self) -> impl Iterator<Item = &ModRef> {
        self.server_mods.iter().chain(self.user_mods.iter())
    }

    // ---- mutators ----------------------------------------------------------

    pub fn set_path(&mut self, new_path: impl Into<String>) {
        self.path = new_path.into();
    }

    pub fn set_server_mods(&mut self, new_server_mods: Vec<String>) {
        self.server_mods = new_server_mods.into_iter().map(ModRef::Url).collect();
    }

    pub fn set_user_mods(&mut self, new_user_mods: Vec<String>) {
        self.user_mods = new_user_mods.into_iter().map(ModRef::Url).collect();
    }

    /// Serialize this config back out to YAML (used by tests and, later,
    /// the dashboard's "personal manifest" endpoint).
    pub fn gen_config_file(&self, filename: &Path) -> Result<(), Box<dyn Error>> {
        let file = File::create(filename)?;
        serde_yaml::to_writer(file, self)?;
        Ok(())
    }

    // ---- launcher profile --------------------------------------------------

    /// Rewrite the installed Fabric profile in `<mc_path>/launcher_profiles.json`:
    /// max-heap sized for this machine, plus the Currspice name and icon.
    ///
    /// Safe to call a second time (the installer runs it every time).
    pub fn edit_profile(&self, alloc: u64) -> Result<(), Box<dyn Error>> {
        let profile_path = PathBuf::from(self.get_path()).join("launcher_profiles.json");
        self.edit_profile_at(&profile_path, alloc)
    }

    /// Profile edit against an explicit file — the real work, and the unit
    /// test seam (no real .minecraft required).
    pub fn edit_profile_at(&self, profile_path: &Path, alloc: u64) -> Result<(), Box<dyn Error>> {
        let profiles_str = fs::read_to_string(profile_path)?;
        let mut profiles: serde_json::Value = serde_json::from_str(&profiles_str)?;

        let fabric_name = fabric_profile_name(self.get_mc_version());
        // Fail loudly if Fabric never made its profile — the old code then
        // wrote `profiles["fabric-loader-<v>"]` from scratch with null
        // sibling fields and the launcher got a broken entry.
        profiles["profiles"].get(&fabric_name).ok_or_else(|| {
            format!(
                "launcher profile '{fabric_name}' is missing from {} — the Fabric step probably didn't complete (run it first)",
                profile_path.display()
            )
        })?;

        let existing_args = profiles["profiles"][&fabric_name]["javaArgs"]
            .as_str()
            .filter(|s| !s.trim().is_empty());
        // The launcher doesn't write javaArgs by default; fall back to our
        // tuned G1GC set (legacy behavior preserved, now a constant).
        let base_args = existing_args.unwrap_or(JVM_ARGS);
        let java_args = build_java_args(base_args, alloc);

        let profile = profiles["profiles"]
            .as_object_mut()
            .ok_or_else(|| format!("{} has no 'profiles' object", profile_path.display()))?;
        let entry = entry_or_insert(profile, &fabric_name);
        entry["javaArgs"] = serde_json::Value::String(java_args);
        entry["icon"] = serde_json::Value::String(String::from(LOGO));
        entry["name"] = serde_json::Value::String(String::from(PROFILE_NAME));

        fs::write(profile_path, serde_json::to_string_pretty(&profiles)?)?;
        Ok(())
    }
}

/// Borrow the profile object, inserting an empty one if absent (the
/// `ok_or` guard above means this insert path is only reachable on
/// hand-edited files — kept so we never panic).
fn entry_or_insert<'a>(
    map: &'a mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> &'a mut serde_json::Value {
    if !map.contains_key(key) {
        map.insert(key.into(), serde_json::json!({}));
    }
    map.get_mut(key).unwrap()
}

/// True when `url` pins a version of the same Minecraft series as
/// `mc_version`, but not `mc_version` itself ("1.21.11" when the manifest
/// says "1.21.1", or the reverse). URL without a same-series version
/// (non MC-versioned jars like the Kotlin runtime) → false (can't disprove).
///
/// Uses whole version tokens, never raw `str::contains` — "1.21.1" is a
/// substring of "1.21.11", which is exactly how the drift hid.
fn conflict_in(url: &str, mc_version: &str) -> bool {
    let Some(series) = series_prefix(mc_version) else {
        return false;
    };
    version_tokens(url)
        .iter()
        .any(|t| t.starts_with(&series) && t != mc_version)
}

/// "1.21.11" → "1.21." (the series to compare within). None for anything
/// that isn't a ≥2-component version.
fn series_prefix(mc_version: &str) -> Option<String> {
    let parts: Vec<&str> = mc_version.split('.').collect();
    if parts.len() >= 2 {
        Some(format!("{}.{}.{}", parts[0], parts[1], ""))
    } else {
        None
    }
}

/// All maximal dotted-number runs in a string, with leading/trailing dots
/// trimmed: "a-1.21.11-b9" → ["1.21.11", "9"],
/// "0.141.2%2B1.21.11" → ["0.141.2", "2", "1.21.11"].
fn version_tokens(s: &str) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_digit() || c == '.' {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(beg) = start {
            push_token(&mut out, &s[beg..i]);
            start = None;
        }
        i += 1;
    }
    if let Some(beg) = start {
        push_token(&mut out, &s[beg..]);
    }
    out
}

fn push_token(out: &mut Vec<String>, run: &str) {
    let t = run.trim_matches('.');
    if !t.is_empty() && t.bytes().any(|b| b.is_ascii_digit()) {
        out.push(t.to_string());
    }
}

/// Replace the `-Xmx…` flag in a JVM argument string, keeping everything
/// else in order; append it if it isn't there.
///
/// Split on **whitespace**, never on `-` — the old implementation split on a
/// hyphen and rejoined with hyphens, which corrupted any argument containing
/// an inner dash.
pub fn build_java_args(existing: &str, alloc: u64) -> String {
    let new_flag = format!("-Xmx{alloc}G");
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    for tok in existing.split_whitespace() {
        if tok.contains("-Xmx") {
            replaced = true;
            continue;
        }
        out.push(tok.to_string());
    }
    if replaced {
        out.push(new_flag);
    } else {
        out.insert(0, new_flag);
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modref_accepts_both_yaml_shapes_and_rejects_bad_hashes() {
        for yaml in [
            "https://host/a.jar".to_string(),
            format!("url: https://host/a.jar\nsha256: {}", "a".repeat(64)),
        ] {
            let item: ModRef = serde_yaml::from_str(&yaml).unwrap();
            item.validate().unwrap();
            assert_eq!(
                item,
                serde_yaml::from_str::<ModRef>(&serde_yaml::to_string(&item).unwrap()).unwrap()
            );
        }
        for hash in ["", " ", "abc", &"z".repeat(64)] {
            assert!(
                ModRef::Entry {
                    url: "https://host/a.jar".into(),
                    sha256: Some(hash.into())
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn config_rejects_bad_entry_checksum() {
        let mut c = sample_config();
        c.server_mods = vec![ModRef::Entry {
            url: "https://host/a.jar".into(),
            sha256: Some("bad".into()),
        }];
        assert!(c.validate().is_err());
    }

    fn sample_config() -> Config {
        Config::new(
            "",
            &[
                "https://cdn.modrinth.com/data/AANobbMI/versions/59wygFUQ/sodium-fabric-0.8.2%2Bmc1.21.11.jar",
                "https://cdn.modrinth.com/data/w7ThoJFB/versions/YphN3CnL/zoomify-2.15.1%2B1.21.11.jar",
            ],
            &["https://cdn.modrinth.com/data/xxx/versions/yyy/soundpack-1.21.11.jar"],
            "1.21.11",
            "0.18.4",
            "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar",
        )
    }

    // ---- build_java_args: the old split-on-dash bug, locked down ---------

    #[test]
    fn java_args_replaces_existing_xmx() {
        let out = build_java_args("-Xmx2G -XX:+UseG1GC", 4);
        assert!(out.contains("-Xmx4G"), "got: {out}");
        assert!(!out.contains("-Xmx2G"), "old Xmx survives: {out}");
        assert!(
            out.contains("-XX:+UseG1GC"),
            "other flags must survive: {out}"
        );
    }

    #[test]
    fn java_args_appends_when_absent() {
        let out = build_java_args("-XX:+UseG1GC", 6);
        assert!(out.contains("-Xmx6G"), "got: {out}");
        assert_eq!(out.matches("-Xmx").count(), 1, "exactly one Xmx: {out}");
    }

    #[test]
    fn java_args_with_empty_string() {
        let out = build_java_args("", 3);
        assert_eq!(out, "-Xmx3G", "got: {out}");
    }

    #[test]
    fn java_args_survives_dash_heavy_flags() {
        // The old splitter mangled arguments whose value has dashes
        // (e.g. -Djdk.net.hosts.file=/etc/hosts, -XX:G1HeapRegionSize=32M).
        let in_args = "-XX:G1HeapRegionSize=32M -Djdk.net.hosts.file=/etc/hosts -Xmx2G";
        let out = build_java_args(in_args, 8);
        assert!(out.contains("-XX:G1HeapRegionSize=32M"), "got: {out}");
        assert!(
            out.contains("-Djdk.net.hosts.file=/etc/hosts"),
            "got: {out}"
        );
        assert!(out.contains("-Xmx8G"), "got: {out}");
        assert_eq!(out.matches("-Xmx").count(), 1);
    }

    // ---- validate ----------------------------------------------------------

    #[test]
    fn validate_accepts_good_manifest() {
        sample_config().validate().expect("sample should be valid");
    }

    #[test]
    fn validate_rejects_missing_version() {
        let mut c = sample_config();
        c.mc_version = String::new();
        let e = c.validate().expect_err("empty mc_version must fail");
        assert!(e.to_string().contains("mc_version"), "got: {e}");
    }

    #[test]
    fn validate_rejects_non_https_mod_link() {
        let mut c = sample_config();
        c.set_server_mods(vec!["ftp://mods.example/jar.jar".into()]);
        let e = c.validate().expect_err("non-https link must fail");
        assert!(e.to_string().contains("https"), "got: {e}");
    }

    #[test]
    fn validate_rejects_non_https_fabric_url() {
        let mut c = sample_config();
        c.fabric = String::from("http://example.com/installer.jar");
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_rejects_empty_mod_list() {
        let mut c = sample_config();
        c.set_server_mods(vec![]);
        assert!(c.validate().is_err());
    }

    // ---- version consistency (the main-branch drift class) ------------------

    #[test]
    fn consistency_passes_when_urls_match_version() {
        sample_config()
            .verify_version_consistency()
            .expect("all sample URLs carry 1.21.11");
    }

    #[test]
    fn consistency_fails_when_version_mixed() {
        // Exactly the pre-release-branch accident: 1.21.1 declared, 1.21.11
        // build URLs. Must fail with a message naming the bad link.
        let mut c = sample_config();
        c.mc_version = String::from("1.21.1");
        let e = c
            .verify_version_consistency()
            .expect_err("mixed versions must not install");
        let msg = e.to_string();
        assert!(msg.contains("1.21.11"), "offender link in message: {msg}");
        assert!(
            msg.contains("manifest declares Minecraft 1.21.1"),
            "names the declared version: {msg}"
        );
    }

    #[test]
    fn consistency_catches_user_mods_too() {
        let mut c = sample_config();
        c.set_user_mods(vec![String::from(
            "https://cdn.modrinth.com/data/zzz/versions/kkk/mod-1.21.1.jar",
        )]);
        assert!(
            c.verify_version_consistency().is_err(),
            "a 1.21.1 build under a 1.21.11 manifest must be flagged"
        );
    }

    // ---- version tokens & conflict detection (the drift-class primitives) --

    #[test]
    fn version_tokens_pull_dotted_runs() {
        assert_eq!(
            version_tokens("0.141.2%2B1.21.11"),
            vec!["0.141.2", "2", "1.21.11"]
        );
        assert_eq!(version_tokens("a-1.21.11-b9"), vec!["1.21.11", "9"]);
        assert_eq!(version_tokens("no-dots-here"), Vec::<String>::new());
    }

    #[test]
    fn conflict_same_series_different_version() {
        // 1.21.1 declared, 1.21.11 built — the pre-release accident.
        assert!(conflict_in(
            "https://cdn.modrinth.com/data/P7dR8mSH/versions/L34yYPTD/fabric-api-0.141.2%2B1.21.11.jar",
            "1.21.1"
        ));
        // and the mirror image: 1.21.11 declared, 1.21.1 built.
        assert!(conflict_in("https://x/mod-1.21.1.jar", "1.21.11"));
        // matching version: not a conflict.
        assert!(!conflict_in("https://x/mod-1.21.11.jar", "1.21.11"));
    }

    #[test]
    fn conflict_rejects_substring_traps() {
        // "1.21.1" is a substring of every "1.21.11" URL — the old
        // naive-not-contains check flagged all of them. Whole-token logic
        // must not.
        assert!(!conflict_in("sodium-0.8.2%2Bmc1.21.11.jar", "1.21.11"));
        // a non MC-versioned jar (Kotlin runtime): can't disprove → passes.
        assert!(!conflict_in(
            "fabric-language-kotlin-1.13.8%2Bkotlin.2.3.0.jar",
            "1.21.11"
        ));
    }

    // ---- round trip ----------------------------------------------------------

    #[test]
    fn roundtrip_yaml_preserves_everything() {
        let c = sample_config();
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("mods.yaml");
        c.gen_config_file(&file).expect("write yaml");
        let loaded = Config::load_new(&file).expect("reload yaml");
        assert_eq!(c, loaded, "serialize/deserialize must be lossless");
    }

    #[test]
    fn load_missing_file_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(Config::load_new(&dir.path().join("nope.yaml")).is_err());
    }

    #[test]
    fn empty_path_and_user_mods_default() {
        // The manifest in the wild has `path:` (empty) and `user_mods:`
        // (empty) — both must deserialize rather than hard-fail.
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("minimal.yaml");
        fs::write(
            &file,
            r#"
mc_version: 1.21.11
installer_version: 0.18.4
fabric: https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar
server_mods:
- https://cdn.modrinth.com/data/x/versions/y/mod-1.21.11.jar
user_mods:
"#,
        )
        .expect("write");
        let c = Config::load_new(&file).expect("minimal manifest must load");
        assert_eq!(c.get_path(), "");
        assert!(c.get_user_mods().is_empty());
        assert_eq!(c.get_server_mods().len(), 1);
    }

    // ---- profile edit --------------------------------------------------------

    fn profile_doc() -> serde_json::Value {
        serde_json::json!({
            "client": { "latest": "1.21.11" },
            "launcherVersion": "1.21.11",
            "profiles": {
                "fabric-loader-1.21.11": {
                    "lastUsed": "2026-08-30T16:53:47-0700",
                    "lastVersionId": "fabric-loader-0.18.4-1.21.11",
                    "name": "fabric-loader-1.21.11",
                    "type": "custom"
                }
            }
        })
    }

    fn write_profiles(dir: &Path) -> PathBuf {
        let p = dir.join("launcher_profiles.json");
        fs::write(&p, serde_json::to_string_pretty(&profile_doc()).unwrap()).unwrap();
        p
    }

    #[test]
    fn profile_edit_sets_name_icon_and_ram() {
        let c = sample_config();
        let dir = tempfile::tempdir().expect("tempdir");
        let file = write_profiles(dir.path());

        c.edit_profile_at(&file, 8).expect("edit profile");

        let doc =
            serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&file).unwrap()).unwrap();
        let prof = &doc["profiles"]["fabric-loader-1.21.11"];
        assert_eq!(prof["name"], crate::PROFILE_NAME);
        assert!(
            prof["icon"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert!(prof["javaArgs"].as_str().unwrap().contains("-Xmx8G"));
        // untouched fields preserved
        assert_eq!(prof["lastVersionId"], "fabric-loader-0.18.4-1.21.11");
    }

    #[test]
    fn profile_edit_appends_default_jvm_when_no_javaargs() {
        // The Fabric CLI creates profiles with no javaArgs — the legacy
        // fallback constant must still be used.
        let c = sample_config();
        let dir = tempfile::tempdir().expect("tempdir");
        let file = write_profiles(dir.path());

        c.edit_profile_at(&file, 4).expect("edit profile");

        let doc =
            serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&file).unwrap()).unwrap();
        let args = doc["profiles"]["fabric-loader-1.21.11"]["javaArgs"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(args.contains("-Xmx4G"), "ram flag present: {args}");
        assert!(
            args.contains("UseG1GC"),
            "default JVM tuning present: {args}"
        );
    }

    #[test]
    fn profile_edit_missing_profile_is_a_clear_error() {
        let mut c = sample_config();
        c.set_path(dir_str());
        let dir = tempfile::tempdir().expect("tempdir");
        // Fresh profile doc with the version the config expects removed:
        fs::write(
            dir.path().join("launcher_profiles.json"),
            r#"{"profiles": {"other-profile": {}}}"#,
        )
        .unwrap();
        c.edit_profile_at(&dir.path().join("launcher_profiles.json"), 4)
            .expect_err("must not silently create a foreign profile");

        fn dir_str() -> String {
            String::new()
        }
    }

    #[test]
    fn profile_edit_missing_file_is_a_clean_error() {
        let c = sample_config();
        let dir = tempfile::tempdir().expect("tempdir");
        let e = c
            .edit_profile_at(&dir.path().join("nope.json"), 4)
            .expect_err("no file → err");
        assert!(!e.to_string().is_empty());
    }
}
