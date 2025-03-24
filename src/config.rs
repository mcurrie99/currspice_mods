// Standard Library Imports
use std::fs;
use std::error::Error;

// Internal Imports

// TODO: Implement YAML
// Outside Imports
#[allow(unused_imports)]
use serde_yaml as ym;
use serde::{Serialize, Deserialize};

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    // path: Path to players Minecraft Installation File
    // server_mods: List of links to download links for server mods
    // user_mods: List of links to download links for user mods
    path: String,
    server_mods: Vec<String>,
    user_mods: Vec<String>
}

impl Config {
    pub fn new(path:String, server_mods:Vec<String>, user_mods:Vec<String>) -> Config {
        Config {
            path,
            server_mods,
            user_mods
        }
    }

    // Returns Players Minecraft Path
    pub fn get_path(&self) -> &String {
        &self.path
    }
    
    // Returns Mods from Server 
    pub fn get_server_mods(&self) -> &Vec<String> {
        &self.server_mods
    }

    // Returns Mods from User
    pub fn get_user_mods(&self) -> &Vec<String> {
        &self.user_mods
    }

    // Changes players Minecraft install path
    pub fn set_path(&mut self, new_path:String) -> () {
        self.path = new_path;
    }

    // Changes players mods link for server mods
    pub fn set_server_mods(&mut self, new_server_mods:Vec<String>) -> () {
        self.server_mods = new_server_mods;
    }

    // Changes players mods links for user mods
    pub fn set_user_mods(&mut self, new_user_mods:Vec<String>) -> () {
        self.user_mods = new_user_mods;
    }

    pub fn gen_config_file(&self, filename:&str) -> Result<(), Box<dyn Error>> {
        // Returns Configuration file for minecraft install    
        let file = fs::File::create(filename)?;
        serde_yaml::to_writer(file, &self)?;
    
        Ok(())
    }
}


// TODO: Finish tests for Config files
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_sets() {
        // Test Generation of the yaml file
        let mut test_config = Config::new(String::from("Wrong"), vec![String::from("Wrong")], vec![String::from("Wrong")]);

        let test_path = String::from("Correct");
        let test_server_mods = vec![String::from("Server Mod 1"), String::from("Server Mod 2")];
        let test_user_mods = vec![String::from("User Mod 1"), String::from("User Mod 2")];

        // Changes configs
        test_config.set_path(test_path.clone());
        test_config.set_server_mods(test_server_mods.clone());
        test_config.set_user_mods(test_user_mods.clone());

        
        // Checks if values were created properly
        assert_eq!(&test_path, test_config.get_path());
        for (corr, check) in test_server_mods.iter().zip(test_config.get_server_mods().iter()) {
            assert_eq!(corr, check);
        }
        for (corr, check) in test_user_mods.iter().zip(test_config.get_user_mods().iter()) {
            assert_eq!(corr, check);
        }
        

    }

    #[test]
    fn test_config_new() {
        // Test Variables
        let test_path = String::from("Correct");
        let test_server_mods = vec![String::from("Server Mod 1"), String::from("Server Mod 2")];
        let test_user_mods = vec![String::from("User Mods 1"), String::from("User Mods 2")];

        // Creates File
        let config = Config::new(
            test_path.clone(), 
            test_server_mods.clone(), 
            test_user_mods.clone()
        );

        // Checks if values were created properly
        assert_eq!(&test_path, config.get_path());
        for (corr, check) in test_server_mods.iter().zip(config.get_server_mods().iter()) {
            assert_eq!(corr, check);
        }
        for (corr, check) in test_user_mods.iter().zip(config.get_user_mods().iter()) {
            assert_eq!(corr, check);
        }
    }
}
