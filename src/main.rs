use currspice_mods as mods;

use std::thread;
use std::time::Duration;

// #[cfg(not(target_os = "windows"))]
// compile_error!("This application only supports Windows");

// Sets up constant for pulling pods to download.
#[allow(dead_code)]
const CONFIG_URL: &str = "http://server.currspice.com/mods.yaml";

fn main() {
    // Attempts to open configuration file
    let filepath = "./mods.yaml";

    let config = match mods::config::Config::load_new(filepath) {
        Ok(config) => config,
        _ => mods::config::Config::download_new(filepath, CONFIG_URL).unwrap(),
    };

    println!("{:?}", config);

    // Guesses initial path to minecraft install
    // let test = mods::tools::guess_minecraft_dir().unwrap();
    // println!("{test:?}");

    // TODO: TESTING INSTALL CODE
    // -------------------------------------------------------------------------------------
    // Temporary Directory
    let directory = "./mods";

    let mut mods: Vec<currspice_mods::mods::Mod> = Vec::new();
    
    // // Grabs URL
    for (i, link) in config.get_server_mods().iter().enumerate() {
        // Creates Mod Object        
        let mut modder = currspice_mods::mods::Mod::new(link, directory).unwrap();

        println!("Mod {i}: {}", modder.get_name());

        // Downloads mod
        modder.download().unwrap();

        mods.push(modder);
    }

    // Sleeps for 2 seconds
    println!("Sleeping for 2 seconds");
    thread::sleep(Duration::from_secs(2));

    // Deletes files
    println!("Deleting Files");
    for modder in mods.iter_mut() {
        modder.delete().unwrap();
    }
    println!("Successfully deleted files");


    // TODO: Remove when done
    // ---------------------------------------------------------------------------------------
    // let mut response = get(config.get_fabric_url()).unwrap();

    // // TODO: Write code to download file for us
    // let installer = "fabric-installer.jar";
    // let mut installer_file = File::create(installer).unwrap();
    // copy(&mut response, &mut installer_file).unwrap();

    // // Tries to install fabric
    // let mut fabric = mods::mods::Fabric::from_config(installer, &config).unwrap();
    // let _ = fabric.install(config.get_path()).unwrap();

    println!("File downloaded successfully");
}
