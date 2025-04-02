use std::fs::File;
use std::error::Error;
use std::process::{Command, ExitStatus};

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

    // TODO: Write code to Mod file
    pub fn download(&mut self) {

    }

    // TODO: Write code to install Mod file
    pub fn install() {

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
    mc_version: String,
    inst_version: String,
    installed: bool
}

impl Fabric {
    pub fn new(filename:&str, mc_version:&str, inst_version:&str) -> Result<Fabric, Box<dyn Error>> {
        let file = File::open(filename)?;
        
        Ok(Fabric {
            installer: file,
            install_name: String::from(filename),
            mc_version: String::from(mc_version),
            inst_version: String::from(inst_version),
            installed: false
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

    // TODO: Implement downloading for the Fabric installer
    pub fn download(&mut self) {

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
