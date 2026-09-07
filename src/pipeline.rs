//! The CLI and deterministic integration smoke tests use this same pipeline.
use crate::{
    config::Config,
    download::{InstallError, Outcome, Transport},
    mods::{Fabric, Mod},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Default, Debug)]
pub struct Options {
    pub mc_dir: PathBuf,
    pub dry_run: bool,
    pub no_download: bool,
    pub verbose: bool,
    pub heap_gb: u64,
}
/// Only effects requiring installed software are replaced in smoke tests.
pub trait Runtime {
    fn java(&mut self) -> Result<PathBuf, InstallError>;
    fn fabric(&mut self, fabric: &Fabric, java: &Path, verbose: bool) -> Result<(), InstallError>;
}
pub struct SystemRuntime;
impl Runtime for SystemRuntime {
    fn java(&mut self) -> Result<PathBuf, InstallError> {
        crate::jre::ensure_java(true).map_err(|e| {
            if e.downcast_ref::<reqwest::Error>().is_some() {
                InstallError::Network(e.to_string())
            } else {
                InstallError::Install(e.to_string())
            }
        })
    }
    fn fabric(&mut self, fabric: &Fabric, java: &Path, verbose: bool) -> Result<(), InstallError> {
        let args = fabric.installer_command();
        if verbose {
            eprintln!("Java command: {:?} {:?}", java, args);
        }
        let out = std::process::Command::new(java)
            .args(args)
            .output()
            .map_err(|e| InstallError::Install(format!("Could not start Fabric: {e}")))?;
        if verbose {
            eprintln!("{}", String::from_utf8_lossy(&out.stdout));
            eprintln!("{}", String::from_utf8_lossy(&out.stderr));
        }
        if !out.status.success() {
            return Err(InstallError::Install(format!(
                "Fabric installer failed ({}). Re-run with --verbose for its output.",
                out.status
            )));
        }
        Ok(())
    }
}

pub fn install(
    mut config: Config,
    options: &Options,
    http: &mut dyn Transport,
    runtime: &mut dyn Runtime,
) -> Result<String, InstallError> {
    config
        .validate()
        .and_then(|_| config.verify_version_consistency())
        .map_err(|e| InstallError::Config(e.to_string()))?;
    if !options.mc_dir.is_dir() {
        return Err(InstallError::Config(format!(
            "Minecraft directory does not exist: {}",
            options.mc_dir.display()
        )));
    }
    config.set_path(options.mc_dir.to_string_lossy());
    let fabric = Fabric::from_config(&config).map_err(|e| InstallError::Config(e.to_string()))?;
    let mods_dir = options.mc_dir.join("mods");
    let mut names = HashSet::new();
    let mut mods = Vec::new();
    for item in config.mod_refs() {
        let artifact =
            Mod::new(item.url(), &mods_dir).map_err(|e| InstallError::Config(e.to_string()))?;
        if !names.insert(artifact.get_name().to_ascii_lowercase()) {
            return Err(InstallError::Config(format!(
                "Duplicate mod filename: {}",
                artifact.get_name()
            )));
        }
        if options.verbose {
            eprintln!(
                "Mod URL: {}; SHA-256: {}",
                item.url(),
                item.sha256().unwrap_or("UNPINNED (legacy URL)")
            );
        }
        mods.push((artifact, item.sha256()));
    }
    if options.verbose {
        eprintln!(
            "Fabric URL: {} (HTTPS, no manifest checksum)",
            fabric.installer().get_download()
        );
    }
    if options.dry_run {
        return Ok(format!(
            "DRY RUN — nothing was changed. Fabric client {} / loader {}; {} mod(s) -> {}; heap {} GB",
            config.get_mc_version(),
            config.get_installer_version(),
            mods.len(),
            mods_dir.display(),
            options.heap_gb
        ));
    }
    if options.no_download {
        return Err(InstallError::Install("Offline Fabric dependency cache cannot yet be fully verified. No files changed; use --dry-run to inspect the plan, or re-run without --no-download.".into()));
    }
    let java = runtime.java()?;
    let mut installer = Mod::new(fabric.installer().get_download(), &options.mc_dir)
        .map_err(|e| InstallError::Config(e.to_string()))?;
    installer.install(None, http)?;
    fabric
        .seed_launcher_profile()
        .map_err(|e| InstallError::Install(e.to_string()))?;
    runtime.fabric(&fabric, &java, options.verbose)?;
    fabric.verify_profile()?;
    config
        .edit_profile(options.heap_gb.max(1))
        .map_err(|e| InstallError::Install(e.to_string()))?;
    std::fs::create_dir_all(&mods_dir)?;
    let (mut installed, mut replaced, mut skipped) = (0, 0, 0);
    for (mut artifact, hash) in mods {
        match artifact.install(hash, http)? {
            Outcome::Installed => installed += 1,
            Outcome::Replaced => replaced += 1,
            Outcome::Skipped => skipped += 1,
        }
    }
    fabric.verify_profile()?;
    Ok(format!(
        "Installed to: {}\nFabric client {} / loader {}\nMods installed: {installed}; replaced: {replaced}; skipped: {skipped}\nMods folder: {}\nLauncher profile: {} ({} GB heap)",
        options.mc_dir.display(),
        config.get_mc_version(),
        config.get_installer_version(),
        mods_dir.display(),
        crate::PROFILE_NAME,
        options.heap_gb.max(1)
    ))
}
