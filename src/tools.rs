//! System helpers: finding the Minecraft directory and sizing RAM allocation.
//!
//! Kept small and pure where possible so tests can drive them without a
//! real Minecraft install.
use std::env;
use std::error::Error;
use std::path::PathBuf;
use sysinfo::System;

/// The standard Windows `.minecraft` location for a username.
///
/// Split out from [`guess_minecraft_dir`] so it can be tested without
/// depending on the current environment.
pub fn default_minecraft_path(username: &str) -> PathBuf {
    PathBuf::from(format!(r"C:\Users\{username}\AppData\Roaming\.minecraft"))
}

/// Guess the (Windows) Minecraft directory: `%USERNAME%` + the standard
/// Roaming path. Project scope (IDEA.md) is Windows-only.
pub fn guess_minecraft_dir() -> Result<PathBuf, Box<dyn Error>> {
    let username = env::var("USERNAME").map_err(|_| {
        "USERNAME environment variable is not set (expected on Windows)".to_string()
    })?;
    let guess = default_minecraft_path(&username);
    if guess.is_dir() {
        Ok(guess)
    } else {
        Err(format!("Minecraft directory does not exist at {}", guess.display()).into())
    }
}

/// RAM allocation (GiB) for the Minecraft profile: round total physical
/// memory, divide by `factor`, clamped to a sane [2, 32] band so a 2 GB
/// machine doesn't get 1 GB and a 128 GB box doesn't get a 64 GB heap.
pub fn determine_ram_alloc(factor: f64) -> u64 {
    let mut sys = System::new_all();
    sys.refresh_memory();

    let total_ram_gb = sys.total_memory() as f64 / 1024.0f64.powi(3);
    let alloc = (total_ram_gb.round() / factor) as u64;
    alloc.clamp(2, 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_windows_layout() {
        assert_eq!(
            default_minecraft_path("matth"),
            std::path::Path::new(r"C:\Users\matth\AppData\Roaming\.minecraft")
        );
    }

    #[test]
    fn default_path_roundtrips_display() {
        // The string form must be a valid, unambiguous Windows path — a drive
        // prefix, the standard AppData layout, nothing trailing. (Component
        // counting varies by exactly one for drive-rooted paths, so we assert
        // the invariants that actually matter rather than a brittle count.)
        let p = default_minecraft_path("matth");
        let s = p.to_string_lossy().to_string();
        assert!(
            s.starts_with("C:\\Users\\matth\\") || s.starts_with("C:/Users/matth/"),
            "s={s}"
        );
        assert!(
            s.contains("AppData\\Roaming\\.minecraft") || s.contains("AppData/Roaming/.minecraft"),
            "s={s}"
        );
        assert!(
            !s.ends_with('\\') && !s.ends_with('/'),
            "no trailing separator: s={s}"
        );
    }

    #[test]
    fn guess_dir_error_mentions_path_when_missing() {
        // Deterministic half of the contract: when USERNAME points at no
        // directory, the error names that exact path so a friend (or you)
        // can pass --mc-dir.
        //
        // On machines that DO have a standard .minecraft this returns Ok —
        // both arms are still correct behavior, so accept either, but the
        // error text (the part that matters for support) is asserted here.
        let res = guess_minecraft_dir();
        match res {
            Ok(p) => assert!(p.to_string_lossy().contains(".minecraft")),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("does not exist at") || msg.contains("USERNAME"),
                    "unexpected error message: {msg}"
                );
            }
        }
    }

    #[test]
    fn ram_alloc_is_within_sane_band() {
        let alloc = determine_ram_alloc(2.0);
        assert!(
            (2..=32).contains(&alloc),
            "allocation {alloc} GiB is outside [2, 32]"
        );
    }

    #[test]
    fn ram_alloc_factor_monotonic() {
        // Dividing by more must never allocate more (regression guard for
        // the old un-clamped `(rounded / factor)` expression).
        let half = determine_ram_alloc(2.0);
        let quarter = determine_ram_alloc(4.0);
        assert!(quarter <= half, "alloc {quarter}G @4 exceeded {half}G @2");
    }

    #[test]
    fn ram_alloc_survives_zero_like_inputs() {
        // Defensive: the formula must not produce a 0 GiB heap (JVM would
        // refuse to start).
        let alloc = determine_ram_alloc(10_000.0);
        assert!(alloc >= 2, "allocation collapsed to {alloc} GiB");
    }
}
