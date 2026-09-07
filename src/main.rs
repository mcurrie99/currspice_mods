//! Currspice — one-click Minecraft + mod installer for the Currspice SMP.
//!
//! Phases (ROADMAP.md): load/verify manifest → resolve the .minecraft dir →
//! headless Fabric install → size the launcher profile → download mods →
//! print a human summary. Exit codes: 0 ok · 1 config · 2 network ·
//! 3 install · 4 verify.
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use clap::Parser;

use currspice_mods::config::Config;
use currspice_mods::mods::{Fabric, Mod};
use currspice_mods::tools::{determine_ram_alloc, guess_minecraft_dir};

/// Currspice — one-click Minecraft mod installer for the SMP.
#[derive(Parser, Debug)]
#[command(name = "currspice_mods", version, about)]
struct Cli {
    /// Path to your Minecraft (.minecraft) directory
    #[arg(long, value_name = "PATH")]
    mc_dir: Option<PathBuf>,

    /// Path to a mods.yaml manifest (default: ./mods.yaml, else the hosted one)
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Print what would happen without changing anything
    #[arg(long)]
    dry_run: bool,

    /// Verbose output (raw commands, URLs)
    #[arg(long)]
    verbose: bool,

    /// Never download anything (not even a private Java copy)
    #[arg(long)]
    no_download: bool,
}

/// Exit-code contract (structured failure — no guessing from prose).
const EXIT_CONFIG: i32 = 1;
const EXIT_NETWORK: i32 = 2;
const EXIT_INSTALL: i32 = 3;
const EXIT_VERIFY: i32 = 4;

fn main() {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(summary) => {
            println!();
            println!("{summary}");
            if !cli.dry_run {
                println!("Currspice: done. See you in the game!");
            }
        }
        Err((code, msg)) => {
            eprintln!();
            eprintln!("Currspice: {msg}");
            if !cli.verbose {
                eprintln!("Currspice: re-run with --verbose for the full detail.");
            }
            std::process::exit(code);
        }
    }
    pause_if_interactive();
}

/// Old `end()` behavior, only when a human is actually attached to the
/// console (double-click in Explorer). Never blocks scripted/CI runs.
fn pause_if_interactive() {
    use std::io::IsTerminal;
    if !io::stdin().is_terminal() {
        return;
    }
    eprintln!("[Currspice] Press Enter to close this window...");
    let _ = io::stdout().flush();
    let mut _s = String::new();
    let _ = io::stdin().read_line(&mut _s);
}

fn run(cli: &Cli) -> Result<String, (i32, String)> {
    // 1. Manifest: local file first, fall back to the hosted one.
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("mods.yaml"));
    let mut config: Config = match Config::load_new(&config_path) {
        Ok(c) => c,
        Err(_) => {
            println!(
                "Currspice: no local manifest — downloading the current one from {}",
                currspice_mods::CONFIG_URL
            );
            Config::download_new(&config_path, currspice_mods::CONFIG_URL).map_err(|e| {
                (
                    EXIT_NETWORK,
                    format!(
                        "could not download the manifest from {}:\n{e}\nCheck your internet connection (or pass a local one with --config) and re-run.",
                        currspice_mods::CONFIG_URL
                    ),
                )
            })?
        }
    };
    // Phase-0 drift guard: one MC version, all mods must target it.
    config
        .verify_version_consistency()
        .map_err(|e| (EXIT_CONFIG, e.to_string()))?;
    println!(
        "Currspice: manifest OK — Minecraft {}, {}",
        config.get_mc_version(),
        if config.all_mods().count() == 1 {
            String::from("1 mod")
        } else {
            format!("{} mods", config.all_mods().count())
        }
    );

    // 2. Minecraft directory: explicit flag > standard Windows location.
    let mc_dir: PathBuf = match &cli.mc_dir {
        Some(p) => {
            if !p.is_dir() {
                return Err((
                    EXIT_CONFIG,
                    format!(
                        "no Minecraft directory found at {} (double-check the path, or pass --mc-dir again)",
                        p.display()
                    ),
                ));
            }
            p.clone()
        }
        None => guess_minecraft_dir().map_err(|e| {
            (
                EXIT_CONFIG,
                format!("{e}\nTip: if yours lives elsewhere, pass it with --mc-dir."),
            )
        })?,
    };
    config.set_path(mc_dir.display().to_string());
    println!("Currspice: Minecraft directory: {}", mc_dir.display());

    if cli.dry_run {
        let count = config.all_mods().count();
        let alloc = determine_ram_alloc(2.0);
        return Ok(format!(
            "DRY RUN — nothing was changed.\nWould install:\n  - Fabric (client {} / loader {})\n  - launcher profile with up to {} GB heap, named \"{}\"\n  - {} mod(s) into {}/mods/",
            config.get_mc_version(),
            config.get_installer_version(),
            alloc,
            currspice_mods::PROFILE_NAME,
            count,
            mc_dir.display()
        ));
    }

    // 3. Fabric: own Java → download installer → seed profile → run → verify.
    let mut fabric = Fabric::from_config(&config).map_err(|e| {
        (
            EXIT_CONFIG,
            format!("could not set up the Fabric install: {e}"),
        )
    })?;
    println!(
        "Currspice: installing Fabric (client {} / loader {})",
        fabric.get_mc_version(),
        fabric.get_inst_version()
    );
    let allow_download = !cli.no_download;
    let java_bin = currspice_mods::jre::ensure_java(allow_download)
        .map_err(|e| (EXIT_INSTALL, e.to_string()))?;
    fabric.download().map_err(|e| {
        (
            EXIT_NETWORK,
            format!("could not download the Fabric installer: {e}\nThat's a network problem on our side of things — re-run in a minute."),
        )
    })?;
    fabric.seed_launcher_profile().map_err(|e| {
        (
            EXIT_INSTALL,
            format!("could not prepare the launcher profile file: {e}"),
        )
    })?;
    fabric
        .run_installer(&java_bin)
        .map_err(|e| (EXIT_INSTALL, e.to_string()))?;
    println!("Currspice: Fabric profile installed");

    // 4. Launcher profile: size the heap, brand the profile.
    let alloc = determine_ram_alloc(2.0);
    config.edit_profile(alloc).map_err(|e| {
        (
            EXIT_INSTALL,
            format!("could not update the launcher profile: {e}"),
        )
    })?;
    println!("Currspice: launcher profile set ({alloc} GB heap, name + icon)");

    // 5. Mods: server mods, then user mods. Idempotent by presence.
    let mods_dir = mc_dir.join("mods");
    fs::create_dir_all(&mods_dir).map_err(|e| {
        (
            EXIT_INSTALL,
            format!("could not create the mods folder: {e}"),
        )
    })?;

    let links: Vec<_> = config.all_mods().collect();
    let total = links.len();
    let mut installed_now = Vec::new();
    let mut already_ok = Vec::new();
    for (i, link) in links.iter().enumerate() {
        let mut modder = Mod::new(link, &mods_dir)
            .map_err(|e| (EXIT_CONFIG, format!("could not describe mod {i}: {e}")))?;
        if modder.is_installed() {
            already_ok.push(modder.get_name().to_string());
            continue;
        }
        println!(
            "Currspice: mod {}/{} — downloading {}",
            i + 1,
            total,
            modder.get_name()
        );
        modder.download().map_err(|e| {
            (
                EXIT_NETWORK,
                format!("download failed for {}:\n{e}\nRe-run the installer and it will skip what's already there.", modder.get_name()),
            )
        })?;
        installed_now.push(modder.get_name().to_string());
    }

    // 6. Tidy up the Fabric installer jar.
    fabric.delete().map_err(|e| {
        (
            EXIT_VERIFY,
            format!("could not clean up the Fabric installer: {e}"),
        )
    })?;

    // 7. Summary.
    let mut summary = String::new();
    let _ = writeln!(summary, "Installed to: {}", mc_dir.display());
    let _ = writeln!(
        summary,
        "  - Fabric (client {} / loader {})",
        config.get_mc_version(),
        config.get_installer_version()
    );
    let _ = writeln!(
        summary,
        "  - Launcher profile: \"{}\" (up to {alloc} GB)",
        currspice_mods::PROFILE_NAME
    );
    for name in &installed_now {
        let _ = writeln!(summary, "  - mod installed: {name}");
    }
    for name in &already_ok {
        let _ = writeln!(summary, "  - mod already present: {name}");
    }
    let _ = writeln!(summary, "Mods folder: {}", mods_dir.display());
    Ok(summary)
}
