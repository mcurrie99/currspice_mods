use currspice_mods as mods;
use reqwest::blocking::get;
use std::fs::File;
use std::io::copy;

// #[cfg(not(target_os = "windows"))]
// compile_error!("This application only supports Windows");

fn main() {
    let config = mods::config::Config::load_new("mods.yaml").unwrap();

    println!("{:?}", config);

    // let test = mods::tools::guess_minecraft_dir().unwrap();

    // Grabs URL
    let url = config.get_server_mods().get(0).unwrap();
    println!("Url: {url}");


    let mut response = get(url).unwrap();

    let mut file = File::create("downloaded_file.jar").unwrap();

    copy(&mut response, &mut file).unwrap();

    println!("File downloaded successfully");
}
