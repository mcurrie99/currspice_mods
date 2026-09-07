//! Shared test fixtures: a real (loopback) HTTP server, in-memory zip builders,
//! and launcher-profile helpers. Keep this dependency-light: everything here is
//! std + the crate's existing dev dependencies, so `cargo test` stays hermetic
//! and fast (no DNS, no external hosts, no real Java).
#![allow(unused)]
use std::io::Write;

/// A `Transport` that serves bytes from in-memory bodies without sockets.
/// `bodies[0]` is returned on the first call, `bodies[1]` on the second, and
/// so on; when the queue is exhausted the next call fails with a Network
/// error (so tests can assert "no further download happened").
pub struct Scripted {
    pub calls: Vec<String>, // URLs requested, in order
    bodies: std::collections::VecDeque<std::result::Result<Vec<u8>, String>>,
}

impl Scripted {
    pub fn new(bodies: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            calls: Vec::new(),
            bodies: bodies.into_iter().map(Ok).collect(),
        }
    }
    /// Make the *next* fetch fail with this message (e.g. simulate a network
    /// drop after a couple of good responses).
    pub fn fail_next(&mut self, msg: &str) {
        self.bodies.push_back(Err(msg.to_string()));
    }
    pub fn reset(&mut self, bodies: impl IntoIterator<Item = Vec<u8>>) {
        self.calls.clear();
        self.bodies = bodies.into_iter().map(Ok).collect();
    }
}

impl currspice_mods::download::Transport for Scripted {
    fn fetch(
        &mut self,
        url: &str,
        out: &mut dyn std::io::Write,
    ) -> Result<(), currspice_mods::download::InstallError> {
        use currspice_mods::download::InstallError;
        self.calls.push(url.to_string());
        match self.bodies.pop_front() {
            Some(Ok(b)) => {
                out.write_all(&b)?;
                Ok(())
            }
            Some(Err(e)) => Err(InstallError::Network(e)),
            None => Err(InstallError::Network(
                "script exhausted — no response defined".into(),
            )),
        }
    }
}

// ---- in-memory zip builders -------------------------------------------------

/// A minimal valid jar (a zip with a MANIFEST.MF entry).
pub fn make_jar() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    w.start_file(
        "META-INF/MANIFEST.MF",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    w.write_all(b"Manifest-Version: 1.0\n").unwrap();
    w.finish().unwrap().into_inner()
}

/// A zip with exactly the given entries: `("path", bytes)` pairs.
pub fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(bytes).unwrap();
    }
    w.finish().unwrap().into_inner()
}

/// Adoptium-style nested JRE layout: `jdk-17.0.9+9/bin/java` + a lib file.
pub fn make_jre_zip() -> Vec<u8> {
    make_zip(&[
        ("jdk-17.0.9+9/release", b"JAVA_VERSION=\"17.0.9+9\""),
        ("jdk-17.0.9+9/bin/java", b"#!/bin/sh\necho 'openjdk version \"17.0.9+9\"'"),
        ("jdk-17.0.9+9/lib/modules", b"stub-jimage"),
    ])
}

// ---- launcher profile helpers ----------------------------------------------

/// The launcher_profiles.json shape the real Fabric CLI produces, as JSON.
/// `extra_profile_fields` is merged into the fabric-loader profile entry
/// (e.g. `{}` for the bare case, or `{"name":"x"}` when testing preservation).
pub fn profile_json(mc: &str, inst: &str, extra_profile_fields: &str) -> String {
    let mut profile = serde_json::json!({
        "lastUsed": "2026-01-01T00:00:00Z",
        "lastVersionId": format!("fabric-loader-{inst}-{mc}"),
        "type": "custom",
    });
    let extra: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(extra_profile_fields).unwrap_or_default();
    for (k, v) in extra {
        profile[k] = v;
    }
    serde_json::json!({
        "client": { "latest": mc },
        "launcherVersion": mc,
        "profiles": { format!("fabric-loader-{mc}"): profile },
    })
    .to_string()
}

pub fn write_profiles(root: &std::path::Path, mc: &str, inst: &str, extra: &str) -> std::path::PathBuf {
    let p = root.join("launcher_profiles.json");
    std::fs::write(&p, profile_json(mc, inst, extra)).unwrap();
    p
}

pub fn read_profiles(root: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(root.join("launcher_profiles.json")).unwrap()).unwrap()
}

// ---- valid manifest fixtures --------------------------------------------------

/// A config that passes every validation + consistency check, in-memory.
pub fn valid_config() -> currspice_mods::config::Config {
    currspice_mods::config::Config::new(
        "",
        &["https://cdn.modrinth.com/data/AANobbMI/versions/59wygFUQ/sodium-fabric-0.8.2%2Bmc1.21.11.jar"],
        &["https://cdn.modrinth.com/data/w7ThoJFB/versions/YphN3CnL/zoomify-2.15.1%2B1.21.11.jar"],
        "1.21.11",
        "0.18.4",
        "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.jar",
    )
}

/// A manifest YAML (string) for file-based tests.
pub fn yaml_manifest(mc: &str, inst: &str, fabric: &str, server_mods: &[&str], user_mods: &[&str]) -> String {
    let mut out = format!("mc_version: {mc}\ninstaller_version: {inst}\nfabric: {fabric}\n");
    out.push_str("server_mods:\n");
    for m in server_mods {
        out.push_str(&format!("- {m}\n"));
    }
    if !user_mods.is_empty() {
        out.push_str("user_mods:\n");
        for m in user_mods {
            out.push_str(&format!("- {m}\n"));
        }
    }
    out
}
