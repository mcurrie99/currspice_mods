// Standard Library Imports
use std::fs::{self, File};
use std::error::Error;
use reqwest::blocking;

// Internal Imports

// Outside Imports
use serde::{Serialize, Deserialize};

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    // path: Path to players Minecraft Installation File
    // server_mods: List of links to download links for server mods
    // user_mods: List of links to download links for user mods
    path: String,
    mc_version: String,
    installer_version: String,
    server_mods: Vec<String>,
    user_mods: Vec<String>,
    fabric: String,
}

impl Config {
    // Generates New Configuation Object
    pub fn new(path:&str, server_mods:&[&str], user_mods:&[&str], mc_version:&str, installer_version:&str, fabric_url:&str) -> Config {
        // Creates Vector of Strings for Server Mods
        let mut server_mods_vec = Vec::with_capacity(server_mods.len());
        for modder in server_mods.iter() {
            server_mods_vec.push(String::from(*modder));
        }


        // Creates Vector of Strings for User Mods
        let mut user_mods_vec = Vec::with_capacity(user_mods.len());
        for modder in user_mods.iter() {
            user_mods_vec.push(String::from(*modder))
        }
        
        // Constructs Configuration Object
        Config {
            path: String::from(path),
            server_mods: server_mods_vec,
            user_mods: user_mods_vec,
            mc_version: String::from(mc_version),
            installer_version: String::from(installer_version),
            fabric: String::from(fabric_url)
        }
    }

    // Loads Configuration File
    pub fn load_new(filepath:&str) -> Result<Config, Box<dyn Error>> {
        // Opens File and reads contents
        let file = File::open(filepath)?;
        let config: Config = serde_yaml::from_reader(file)?;

        // Returns created Object
        Ok(config)
    }

    pub fn download_new(filepath:&str, link:&str) -> Result<Config, Box<dyn Error>> {
        // Downloads File
        let response = blocking::get(link)?;
        let content = response.bytes()?;

        // Install file
        fs::write(filepath, &content)?;
        let config = Config::load_new(filepath)?;
        Ok(config)
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

    // Returns Fabric Installer URL
    pub fn get_fabric_url(&self) -> &String {
        &self.fabric
    }

    // Returns requested minecraft version
    pub fn get_mc_version(&self) -> &String {
        &self.mc_version
    }

    // Returns requested installer version
    pub fn get_installer_version(&self) -> &String {
        &self.installer_version
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
        let file = File::create(filename)?;
        serde_yaml::to_writer(file, &self)?;
    
        Ok(())
    }

    pub fn edit_profile(&self, alloc:u64) -> Result<(), Box<dyn Error>> {
    
        // Determines the profile path
        let profile_path = format!(r"{}\launcher_profiles.json", self.get_path());
        println!("Launcher Profiles Directory: {profile_path}");
        let profiles_str = fs::read_to_string(&profile_path)?;
        let mut profiles: serde_json::Value = serde_json::from_str(&profiles_str)?;

        // Pulls Java Arguments
        // NOTE: By default, the JVM arguments are not included into the launcher profile, so these will need to be added manually
        let fabric_name = format!("fabric-loader-{}", self.mc_version);
        let args_str = profiles["profiles"][&fabric_name]["javaArgs"].as_str().unwrap_or_else(|| crate::JVM_ARGS);

        // Creates new argument
        let new_ram_alloc = format!("Xmx{alloc}G ");
        println!("New Argument Insert: {new_ram_alloc}");

        // Edits argument
        let mut args: Vec<&str> = args_str.split("-").collect();

        // Replaces Value in argument String
        if let Some(arg) = args.iter_mut().find(|arg| arg.contains("Xmx")) {
            *arg = new_ram_alloc.as_str();
        } else {
            // Default use
            args.push(&new_ram_alloc);
        }

        // Rejoins the arguments
        let new_arg = args.join("-");
        println!("New Argument Line: {new_arg}");

        // Inserts new argument back into function
        profiles["profiles"][&fabric_name]["javaArgs"] = serde_json::Value::String(new_arg);

        // Updates Icon and Name
        profiles["profiles"][&fabric_name]["icon"] = serde_json::Value::String(String::from(crate::LOGO));
        profiles["profiles"][&fabric_name]["name"] = serde_json::Value::String(String::from(crate::PROFILE_NAME));

        // Updates file
        let update = serde_json::to_string_pretty(&profiles)?;
        fs::write(&profile_path, update)?;

        // Returns Ok if successful
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
        let mut test_config = Config::new("Wrong", &vec!["Wrong"], &vec!["Wrong"], "Wrong", "Wrong", "Wrong");

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
        let test_server_mods = vec!["Server Mod 1", "Server Mod 2"];
        let test_user_mods = vec!["User Mods 1", "User Mods 2"];

        // Creates File
        let config = Config::new(
            &test_path, 
            &test_server_mods, 
            &test_user_mods,
        "TODO",
             "TODO",
            "TODO",
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
