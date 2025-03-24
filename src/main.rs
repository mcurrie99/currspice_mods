use currspice_mods as mods;

// #[cfg(not(target_os = "windows"))]
// compile_error!("This application only supports Windows");

fn main() {
    let test = mods::tools::guess_minecraft_dir().unwrap();

    println!("{test}");

}
