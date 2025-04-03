use std::fs;
use std::error::Error;
use std::process::{Command, ExitStatus};
use reqwest::blocking;

#[allow(dead_code)]
pub struct Mod {
    // name: Name of the mod
    // download: download link for the mod
    // loc: File of Object
    // installed: boolean representing if the mod has been installed.
    name: String,
    download: String,
    path: String,
    installed: bool,
}

// TODO: Check if download link is valid
impl Mod {
    pub fn new(download:&str, directory:&str) -> Result<Mod, Box<dyn Error>> {
        // Obtains filename
        // This only support .jar files
        let filename = Mod::extract_name(download).unwrap_or_else(|| "Mod File.jar");
        let path = format!("{}/{}", directory, filename);

        // Creates Mod Object
        Ok(Mod {
            name:String::from(filename),
            download:String::from(download),
            path: path,
            installed: false,
        })
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn get_download(&self) -> &String {
        &self.download
    }

    pub fn get_mut_name(&mut self) -> &mut String {
        &mut self.name
    }

    pub fn get_mut_download(&mut self) -> &mut String{
        &mut self.download
    }

    pub fn extract_name(link: &str) -> Option<&str> {
        let filename = link.split('/').last()?;
        Some(filename)
    }

    // Downloads mod and creates file
    pub fn download(&mut self) -> Result<(), Box<dyn Error>> {
        // Attempts to download file
        let response = blocking::get(&self.download)?;
        let content = response.bytes()?;

        // Installs file
        fs::write(&self.path, &content)?;
        self.installed = true;
        Ok(())
    }

    // Deletes mod
    pub fn delete(&mut self) -> Result<(), Box<dyn Error>> {
        // Attempts to remove file
        fs::remove_file(&self.path)?;

        // Sets installed status to false
        self.installed = false;
        Ok(())
    }
}


pub struct Fabric {
    // Object to track Fabric Installer before installing mods
    // installer: File object to installer file
    // install_name: File name in string form
    // mc_version: Version of minecraft of the requested values
    // inst_version: Installer Version of Fabric
    // installed: Has Fabric been successfully installed
  
    mc_version: String,
    inst_version: String,
    fabric: Mod,
}

impl Fabric {
    pub fn new(mc_version:&str, inst_version:&str, download:&str, directory:&str) -> Result<Fabric, Box<dyn Error>> {
        // Creates mod object to handle install
        let modder = Mod::new(download, directory)?;
        
        Ok(Fabric {
            mc_version: String::from(mc_version),
            inst_version: String::from(inst_version),
            fabric: modder,
        })
    }

    pub fn from_config(config:&crate::config::Config) -> Result<Fabric, Box<dyn Error>> {
        // Creates mod object to handle install
        let modder = Mod::new(&config.get_fabric_url(), config.get_path())?;

        // Returns Ok enum if file was successfully accesssed
        Ok(Fabric {
            mc_version: String::from(config.get_mc_version()),
            inst_version: String::from(config.get_installer_version()),
            fabric: modder,
        })
    }

    // Returns reference to the requested minecraft install version
    pub fn get_mc_version(&self) -> &String {
        &self.mc_version
    }

    // Returns installer version
    pub fn get_inst_version(&self) -> &String {
        &self.inst_version
    }

    pub fn run_installer(&mut self, mc_path:&str) -> Result<ExitStatus, Box<dyn Error>> {
        // Runs
        let status = Command::new("java")
        .args(&[
            "-jar",
            &self.fabric.name,
            "client",
            "-dir",
            mc_path,
            "-mcversion",
            &self.mc_version,
            "-loader",
            &self.inst_version,
        ]).status()?;

        // Sets installed to ok
        if status.success() {
            self.fabric.installed = true;
            Ok(status)
        } else {
            Err("Failed to Install Fabric".into())
        }
    }
}

// TODO: Fix Tests
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_mod() {
        let name = "mod.jar";
        let download = "google.com/mod.jar";
        let directory = "C:/test";

        let modder = Mod::new(download, directory).unwrap();
        
        assert_eq!(name, modder.get_name());
        assert_eq!(download, modder.get_download())
    }

    #[test]
    fn test_mod_set() {
        let name = "Google";
        let download = "google.com";
        let directory = "C:/test";

        // Creates Mod Object
        let mut modder = Mod::new("google.com/Wrong.jar", directory).unwrap();

        // Edits Values
        { // Mutates the name of the website
        let mut_name = modder.get_mut_name();
        *mut_name = String::from(name);
        }

        { // Mutesates the name of download
        let mut_down = modder.get_mut_download();
        *mut_down = String::from(download);
        }

        // Asserts to make sure the names have been properly set
        assert_eq!(name, modder.get_name());
        assert_eq!(download, modder.get_download())
    }
}
