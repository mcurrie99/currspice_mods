use currspice_mods::config::Config;
#[test]
fn manifest_selection_never_falls_back_from_a_bad_file() {
    let dir = tempfile::tempdir().unwrap();
    let embedded = Config::select(None, dir.path()).unwrap();
    assert_eq!(embedded.get_mc_version(), "1.21.11");
    assert!(embedded.mod_refs().all(|m| m.sha256().is_some()));
    assert!(Config::select(Some(&dir.path().join("missing")), dir.path()).is_err());
    std::fs::write(dir.path().join("mods.yaml"), "invalid: [").unwrap();
    assert!(Config::select(None, dir.path()).is_err());
    let explicit = dir.path().join("explicit.yaml");
    embedded.gen_config_file(&explicit).unwrap();
    assert_eq!(
        embedded,
        Config::select(Some(&explicit), dir.path()).unwrap()
    );
}
