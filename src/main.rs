//! Currspice — one-click Minecraft + mod installer for the Currspice SMP.
//!
//! Phases (ROADMAP.md): resolve manifest (explicit > CWD > embedded) →
//! validate + version-drift check → resolve .minecraft dir → headless Fabric
//! install → size the launcher profile → download mods (checksum-verified,
//! idempotent) → print a human summary.
//!
//! Exit codes: 0 ok · 1 config · 2 network · 3 install-run · 4 verify.
use std::io;
use std::path::{Path, PathBuf};

use clap::Parser;

use currspice_mods::config::Config;
use currspice_mods::download::HttpTransport;
use currspice_mods::pipeline::{Options, SystemRuntime, install};
use currspice_mods::tools::{determine_ram_alloc, guess_minecraft_dir};

/// Currspice — one-click Minecraft mod installer for the SMP.
#[derive(Parser, Debug)]
#[command(name = "currspice_mods", version, about)]
struct Cli {
    /// Path to your Minecraft (.minecraft) directory
    #[arg(long, value_name = "PATH")]
    mc_dir: Option<PathBuf>,

    /// Path to a mods.yaml manifest (default: ./mods.yaml, else the built-in catalog)
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Print what would happen without changing anything
    #[arg(long)]
    dry_run: bool,

    /// Verbose output (raw commands, URLs, hashes)
    #[arg(long)]
    verbose: bool,

    /// Never download anything (not even a private Java copy); fails before any change
    #[arg(long)]
    no_download: bool,
}

/// Exit-code contract (structured failure — no guessing from prose).
/// `InstallError::exit_code()` produces exactly these values:
/// 0 ok (no error) · 1 config · 2 network · 3 install-run · 4 verify.
const EXIT_CONFIG: i32 = 1;
const EXIT_NETWORK: i32 = 2;

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
    use std::io::{IsTerminal, Write};
    if !io::stdin().is_terminal() {
        return;
    }
    eprintln!("[Currspice] Press Enter to close this window...");
    let _ = io::stdout().flush();
    let mut _s = String::new();
    let _ = io::stdin().read_line(&mut _s);
}

fn run(cli: &Cli) -> Result<String, (i32, String)> {
    let cwd = std::env::current_dir().map_err(|e| {
        (
            EXIT_CONFIG,
            format!("could not determine the current directory: {e}"),
        )
    })?;

    // 1. Manifest: explicit > existing CWD override > built-in catalog
    //    (embedded at build time — the manifest host is not in this path).
    //    A missing or invalid manifest is a config error; it never silently
    //    becomes a different catalog (ROADMAP P1, Phase 1).
    let config = Config::select(cli.config.as_deref(), &cwd).map_err(|e| {
        (
            EXIT_CONFIG,
            format!("could not load the mod manifest: {e}"),
        )
    })?;
    // Phase-0 drift guard: one MC version, all mods must target it.
    config.verify_version_consistency().map_err(|e| (EXIT_CONFIG, e.to_string()))?;
    println!(
        "Currspice: manifest OK — Minecraft {}, {} — {}",
        config.get_mc_version(),
        if config.all_mods().count() == 1 {
            String::from("1 mod")
        } else {
            format!("{} mods", config.all_mods().count())
        },
        manifest_source(cli.config.as_deref(), &cwd),
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
    println!("Currspice: Minecraft directory: {}", mc_dir.display());

    // 3. The production pipeline owns everything with teeth: Fabric install
    //    (seed profile → run headless → verify profile), profile sizing, and
    //    checksum-verified mod installs. Dry-run and offline-safety checks
    //    are inside it, before any side effect.
    let options = Options {
        mc_dir: mc_dir.clone(),
        dry_run: cli.dry_run,
        no_download: cli.no_download,
        verbose: cli.verbose,
        heap_gb: determine_ram_alloc(2.0),
    };
    let mut transport = HttpTransport::new().map_err(|e| {
        (EXIT_NETWORK, format!("could not prepare downloads: {e}"))
    })?;
    let summary = install(config, &options, &mut transport, &mut SystemRuntime)
        .map_err(|e| (e.exit_code(), e.to_string()))?;
    Ok(summary)
}

/// Human label for which manifest a run used (support + logs).
fn manifest_source(explicit: Option<&Path>, cwd: &Path) -> String {
    if let Some(p) = explicit {
        format!("manifest {}", p.display())
    } else if cwd.join("mods.yaml").is_file() {
        format!("local override {}", cwd.join("mods.yaml").display())
    } else {
        String::from("built-in catalog (embedded in this installer release)")
    }
}

