use currspice_mods::mods::Fabric;
use currspice_mods as mods;

use std::thread;
use std::time::Duration;

// #[cfg(not(target_os = "windows"))]
// compile_error!("This application only supports Windows");

// Sets up constant for pulling pods to download.
#[allow(dead_code)]
const CONFIG_URL: &str = r"http://server.currspice.com/mods.yaml";

fn main() {
    // Attempts to open configuration file
    println!("Reading Config File");
    let filepath = r"./mods.yaml";
    let config = match mods::config::Config::load_new(filepath) {
        Ok(config) => config,
        _ => {
            println!("Config File not found, downloading...");
            mods::config::Config::download_new(filepath, CONFIG_URL).unwrap()
        },
    };
    println!("Configuration File Initialized...");

    // Guesses initial path to minecraft install
    // let test = mods::tools::guess_minecraft_dir().unwrap();
    // println!("{test:?}");


    // Fabric Install
    println!("Downloading Fabric...");
    // let mut fabric = Fabric::from_config(&config).unwrap();
    let directory = r"./fabric";
    let mc_version = config.get_mc_version();
    let inst_version = config.get_installer_version();
    let fabric_link = config.get_fabric_url();
    let mut fabric = Fabric::new(mc_version, inst_version, fabric_link, directory).unwrap();
    fabric.download().unwrap();

    // Downloads Mods
    // -------------------------------------------------------------------------------------
    // Temporary Directory
    let mods_directory = "./mods";

    // Creates Array of Mods
    let mut mods: Vec<currspice_mods::mods::Mod> = Vec::with_capacity(config.get_server_mods().len());
    
    // Grabs URL
    for (i, link) in config.get_server_mods().iter().enumerate() {
        // Creates Mod Object        
        let mut modder = currspice_mods::mods::Mod::new(link, mods_directory).unwrap();

        println!("Downloading Mod {i}: {}", modder.get_name());

        // Downloads mod
        modder.download().unwrap();
        mods.push(modder);
    }

    // // Sleeps for 2 seconds
    // println!("Sleeping for 2 seconds");
    // thread::sleep(Duration::from_secs(2));

    // // Deletes files
    // println!("Deleting Files");
    // for modder in mods.iter_mut() {
    //     modder.delete().unwrap();
    // }
    // println!("Successfully deleted files");
    println!("File downloaded successfully");
}
