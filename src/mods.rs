use std::fs::File;
use std::error::Error;
use std::process::{Command, ExitStatus};
use std::io;
use reqwest::blocking;

// TODO: Install Implementation
// It might make sense to move this to Mod and have Fabric hold a Mod object
// interally so that we can just use that with an abstraction to help the install 
// of fabric itself.
pub trait Install {
    fn get_download(&self) -> &String;

    // Add progress bar to this
    fn install(&self, dir:&str) -> Result<(), Box<dyn Error>> {
        // Gets filename from url link
        let filename = self.extract_name()?;

        // Creates filepath and file
        let install_location = format!("{}/{}", dir, filename);
        let mut installer_file = File::create(install_location)?;

        // Tries to download the file
        let mut response = blocking::get(self.get_download())?;
        io::copy(&mut response, &mut installer_file)?;

        Ok(())
    }

    fn extract_name(&self) -> Result<&str, Box<dyn Error>> {
        // Splits link and grabs last value of the vector
        let filename = self.get_download()
            .split('/').last().unwrap();
        Ok(filename)
    }
}

#[allow(dead_code)]
pub struct Mod {
    // name: Name of the mod
    // download: download link for the mod
    // loc: File of Object
    // installed: boolean representing if the mod has been installed.
    name: String,
    download: String,
    loc: Option<File>,
    installed: bool
}

// TODO: Check if download link is valid
impl Mod {
    pub fn new(name:&str, download:&str) -> Mod {
        Mod {
            name:String::from(name),
            download:String::from(download),
            loc:None,
            installed: false,
        }
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn get_download(&self) -> &String {
        &self.download
    }

    pub fn get_loc(&self) -> Option<&File> {
        match &self.loc {
            Some(file) => Some(&file),
            None => None
        }
    }

    pub fn get_mut_name(&mut self) -> &mut String {
        &mut self.name
    }

    pub fn get_mut_download(&mut self) -> &mut String{
        &mut self.download
    }

    pub fn get_mut_loc(&mut self, new_loc:File) {
        self.loc = Some(new_loc)
    }

    pub fn remove_loc(&mut self) {
        self.loc = None
    }
}


pub struct Fabric {
    // Object to track Fabric Installer before installing mods
    // installer: File object to installer file
    // install_name: File name in string form
    // mc_version: Version of minecraft of the requested values
    // inst_version: Installer Version of Fabric
    // installed: Has Fabric been successfully installed
    installer: File,
    install_name: String,
    download: String,
    mc_version: String,
    inst_version: String,
    installed: bool
}

impl Fabric {
    pub fn new(filename:&str, mc_version:&str, inst_version:&str, download:&str) -> Result<Fabric, Box<dyn Error>> {
        let file = File::open(filename)?;
        
        Ok(Fabric {
            installer: file,
            install_name: String::from(filename),
            mc_version: String::from(mc_version),
            inst_version: String::from(inst_version),
            download: String::from(download),
            installed: false,
        })
    }

    pub fn from_config(filename:&str, config:&crate::config::Config) -> Result<Fabric, Box<dyn Error>> {
        // Attempts to open file
        let file = File::open(filename)?;

        // Returns Ok enum if file was successfully accesssed
        Ok(Fabric {
            installer: file,
            install_name: String::from(filename),
            mc_version: String::from(config.get_mc_version()),
            inst_version: String::from(config.get_installer_version()),
            download: String::from(config.get_fabric_url()),
            installed: false,
        })
    }

    // Returns referent to the installer file
    pub fn get_installer(&self) -> &File {
        &self.installer
    }

    // Returns reference to the installer name string
    pub fn get_install_name(&self) -> &String {
        &self.install_name
    }

    // Returns reference to the requested minecraft install version
    pub fn get_mc_version(&self) -> &String {
        &self.mc_version
    }

    // Returns installer version
    pub fn get_inst_version(&self) -> &String {
        &self.inst_version
    }

    // Returns reference to installed boolean
    pub fn get_installed(&self) -> &bool {
        &self.installed
    }

    pub fn install(&mut self, mc_path:&str) -> Result<ExitStatus, Box<dyn Error>> {
        // Runs
        let status = Command::new("java")
        .args(&[
            "-jar",
            &self.install_name,
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
            self.installed = true;
            Ok(status)
        } else {
            Err("Failed to Install Fabric".into())
        }
    }

}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_mod() {
        let name = "Google";
        let download = "google.com";

        let modder = Mod::new(&name, &download);
        
        assert_eq!(name, modder.get_name());
        assert_eq!(download, modder.get_download())
    }

    #[test]
    fn test_mod_set() {
        let name = "Google";
        let download = "google.com";

        // Creates Mod Object
        let mut modder = Mod::new("Wrong", "Wrong");

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
