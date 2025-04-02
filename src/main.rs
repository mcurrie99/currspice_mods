use currspice_mods as mods;
use reqwest::blocking::get;
use std::fs::File;
use std::io::copy;

// #[cfg(not(target_os = "windows"))]
// compile_error!("This application only supports Windows");

// Sets up constant for pulling pods to download.
#[allow(dead_code)]
const CONFIG_URL: &str = "http://server.currspice.com/mods.yaml";

fn main() {
    let config = mods::config::Config::load_new("mods.yaml").unwrap();
    println!("{:?}", config);

    // Guesses initial path to minecraft install
    // let test = mods::tools::guess_minecraft_dir().unwrap();
    // println!("{test}");

    // Grabs URL
    for (i, modder) in config.get_server_mods().iter().enumerate() {
        println!("Mod {i}: {modder}");
    }

    // TEST SECTION
    // ---------------------------------------------------------------------------------------
    let mut response = get(config.get_fabric_url()).unwrap();

    // TODO: Write code to download file for us
    let installer = "fabric-installer.jar";
    let mut installer_file = File::create(installer).unwrap();
    copy(&mut response, &mut installer_file).unwrap();

    // Tries to install fabric
    let mut fabric = mods::mods::Fabric::from_config(installer, &config).unwrap();
    let _ = fabric.install(config.get_path()).unwrap();

    println!("File downloaded successfully");
}
