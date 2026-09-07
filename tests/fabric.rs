use currspice_mods::mods::Fabric;
#[test]
fn legacy_exe_becomes_headless_jar_only_for_fabric_maven() {
    let dir = tempfile::tempdir().unwrap();
    let f = Fabric::new(
        "1.21.11",
        "0.18.4",
        "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.0.3/fabric-installer-1.0.3.exe",
        dir.path(),
    )
    .unwrap();
    assert!(f.installer().get_download().ends_with(".jar"));
    assert!(f.installer_command()[1].ends_with(".jar"));
    assert!(
        Fabric::new(
            "1",
            "2",
            "https://evil.test/fabric-installer.exe",
            dir.path()
        )
        .is_err()
    );
}
#[test]
fn profile_verification_requires_actual_matching_profile() {
    let dir = tempfile::tempdir().unwrap();
    let f = Fabric::new("1.21.11", "0.18.4", "https://host/f.jar", dir.path()).unwrap();
    let p = dir.path().join("launcher_profiles.json");
    for text in [
        r#"{"note":"fabric-loader-1.21.11", "profiles":{}}"#,
        r#"{"profiles":{"fabric-loader-1.21.11":null}}"#,
        r#"{"profiles":{"fabric-loader-1.21.11":{"lastVersionId":"fabric-loader-wrong"}}}"#,
    ] {
        std::fs::write(&p, text).unwrap();
        assert_eq!(f.verify_profile().unwrap_err().exit_code(), 4);
    }
    std::fs::write(&p, r#"{"profiles":{"fabric-loader-1.21.11":{"lastVersionId":"fabric-loader-0.18.4-1.21.11"}}}"#).unwrap();
    f.verify_profile().unwrap();
}
