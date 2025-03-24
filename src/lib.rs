pub mod config;
pub mod mods;

pub mod tools {
    // Standard Library Imports
    use std::path;
    use std::env;
    use std::error::Error;


    // Guesses Minecraft Directory
    #[allow(dead_code)]
    pub fn guess_minecraft_dir() -> Result<String, Box<dyn Error>> {
            // Checks that Windows is the OS
            let username = env::var("USERNAME")?;

            // Use different environment variables based on the OS
            let guess = format!(r"C:\Users\{username}\AppData\Roaming\.minecraft");

            // Checks existance of directory
            let check = path::Path::new(&guess);
            if check.exists() && check.is_dir() {
                Ok(guess)
            } else {
                Err(format!("Minecraft Directory does not exist at {}", guess).into())
            }
    }
}