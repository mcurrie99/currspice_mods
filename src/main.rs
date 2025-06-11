// Crate Imports
use currspice_mods::mods::Fabric;
use currspice_mods as mods;

// Standard Library Imports
use std::fs;
use std::process;
use std::path;
use std::io::{self, Write};

// Sets up constant for pulling pods to download.
#[allow(dead_code)]

// TODO: List for getting release build done
// - Implement selecting directory (maybe should be its own ticket)
// - Implement checking if directory exists
// - Install everything in correct locations

// fn main() {
//     // Attempts to open configuration file
//     println!("Currspice: Reading Config File");
//     let filepath = r"./mods.yaml";
//     let mut config = match mods::config::Config::load_new(filepath) {
//         Ok(config) => config,
//         _ => {
//             println!("Currspice: Config File not found, downloading...");
//             match mods::config::Config::download_new(filepath, currspice_mods::CONFIG_URL) {
//                 Ok(config) => config,
//                 _ => {
//                     println!("Currspice: Could not download configuration file");
//                     end();
//                     process::exit(1)/
//                 },
//             }
//         },
//     };
//     println!("Currspice: Configuration File Initialized...");

//     // Guesses directory and tries to open it
//     let minecraft = match mods::tools::guess_minecraft_dir() {
//         Ok(dir) => dir,
//         _ => {
//             println!("Currspice: Non-default minecraft directory detected, create issue ticket");
//             end();
//             process::exit(1)
//         },
//     };
//     config.set_path(minecraft);

//     // Fabric Install
//     println!("Currspice: Downloading Fabric...");
//     let mut fabric =  match Fabric::from_config(&config){
//         Ok(fabric) => fabric,
//         _ => {
//             println!("Currspice: Could not create Fabric Object");
//             end();
//             process::exit(1)
//         }
//     };

//     // Attempts to download fabric installer
//     match fabric.download() {
//         Ok(_) => (),
//         Err(e) => {
//             println!("Currspice: Could not download fabric file");
//             println!("Currspice ERROR: {}", e);
//             end();
//             process::exit(1)
//         }
//     };
    
//     // Runs Fabric Installer
//     match fabric.run_installer(config.get_path()) {
//         Ok(_) => (),
//         Err(e) => {
//             println!("Currspice ERROR: {}", e);
//             println!("Currspice: Could not run installer");
//             end();
//             process::exit(1)
//         }
//     };

//     // Updates Fabric Installer
//     println!("Currspice: Updated Launcher Profile Settings (NOT RUNNING)");
//     let mc_alloc = currspice_mods::tools::determine_ram_alloc(2.0);
//     match config.edit_profile(mc_alloc) {
//         Ok(_) => (),
//         Err(e) => {
//             println!("Currspice ERROR: {}", e);
//             println!("Currspice: Could not edit launcher profile json file.");
//             end();
//             process::exit(1);
//         }
//     }

//     // Downloads Mods
//     // -------------------------------------------------------------------------------------
//     // Mods Directory
//     let mods_directory = format!(r"{}\{}", config.get_path(), "mods");

//     // Creates Array of Mods
//     let mut mods: Vec<currspice_mods::mods::Mod> = Vec::with_capacity(config.get_server_mods().len());

//     // TODO: Move to own function or check for installer
//     let test = path::Path::new(&mods_directory).exists();

//     // Checks to see if mods folder exists and if not will create folder
//     if !test {
//         match fs::create_dir(&mods_directory) {
//             Ok(_) => (),
//             _ => {
//                 println!("Currspice: Could not create mods directory");
//                 end();
//                 process::exit(1)
//             }
//        };
//     }
    
//     // Grabs URL
//     for (i, link) in config.get_server_mods().iter().enumerate() {
//         // Creates Mod Object     
//         let mut modder = match currspice_mods::mods::Mod::new(link, &mods_directory) {
//             Ok(modder) => modder,
//             _ => {
//                 println!("Currspice: Could not crate mod object");
//                 end();
//                 process::exit(1);
//             }
//         };
//         println!("Currspice Downloading Mod {i}: {}", modder.get_name());

//         // Downloads and installes mod
//         match modder.download() {
//             Ok(_) => (),
//             _ => {
//                 println!("Currspice: Could not download file");
//                 end();
//                 process::exit(1)
//             }
//         };

//         // Adds mod to vector
//         mods.push(modder);
//     }

//     // Deletes farbic installer
//     match fabric.delete() {
//         Ok(_) => (),
//         _ => {
//             println!("Currspice: Failed to delete fabric installer");
//             process::exit(1)
//         }
//     };

//     println!("Currspice: File downloaded successfully");

//     // Finishes up process to let user read console
//     end();

// }

// fn end() {
//     println!("Press Enter key to continue...");
//     io::stdout().flush().unwrap();
//     let mut input = String::new();
//     io::stdin().read_line(&mut input).unwrap();
// }

fn main() {
//   let _ = install_java_linux();
}

fn run(cmd: &[&str]) -> anyhow::Result<()> {
    let status = process::Command::new(cmd[0]).args(&cmd[1..]).status()?;
    if !status.success() {
        anyhow::bail!("command {:?} failed", cmd);
    }
    Ok(())
}



