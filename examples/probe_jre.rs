// One-off: exercise the real "bring my own Java" path end to end.
fn main() {
    let t = std::env::temp_dir().join("jre_probe");
    let _ = std::fs::remove_dir_all(&t);
    let home = t.join("jre17");
    let java = currspice_mods::jre::install_jre(&home).expect("private JRE install");
    println!("JRE installed at: {}", java.display());
    let out = std::process::Command::new(&java)
        .arg("-version")
        .output()
        .expect("java runs");
    let version = String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("unknown").to_owned();
    println!("java reports: {version}");
    assert!(
        version.contains("17"),
        "expected a Java 17 runtime, got: {version}"
    );
    println!("PRIVATE_JRE_OK");
}
