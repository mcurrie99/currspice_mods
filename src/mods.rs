use std::fs::File;

#[allow(dead_code)]
pub struct Mod {
    name: String,
    download: String,
    loc: Option<File>,
}

// TODO: Check if download link is valid
impl Mod {
    pub fn new(name:&str, download:&str) -> Mod {
        Mod {
            name:String::from(name),
            download:String::from(download),
            loc:None,
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

    pub fn set_name(&mut self, new_name:&str) {
        self.name = String::from(new_name);
    }

    pub fn set_download(&mut self, new_download:&str) {
        self.download = String::from(new_download);
    }

    pub fn set_loc(&mut self, new_loc:File) {
        self.loc = Some(new_loc)
    }

    pub fn remove_loc(&mut self) {
        self.loc = None
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
        modder.set_name(name);
        modder.set_download(download);
        
        assert_eq!(name, modder.get_name());
        assert_eq!(download, modder.get_download())
    }
}
