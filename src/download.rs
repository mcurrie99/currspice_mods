//! Typed errors and injectable streaming HTTP. Production only accepts HTTPS.
use sha2::{Digest, Sha256};
use std::{
    fmt,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

#[derive(Debug)]
pub enum InstallError {
    Config(String),
    Network(String),
    Install(String),
    Verify(String),
}
impl InstallError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Config(_) => 1,
            Self::Network(_) => 2,
            Self::Install(_) => 3,
            Self::Verify(_) => 4,
        }
    }
}
impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (Self::Config(s) | Self::Network(s) | Self::Install(s) | Self::Verify(s)) = self;
        f.write_str(s)
    }
}
impl std::error::Error for InstallError {}
impl From<std::io::Error> for InstallError {
    fn from(e: std::io::Error) -> Self {
        Self::Install(e.to_string())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Installed,
    Replaced,
    Skipped,
}

pub trait Transport {
    fn fetch(&mut self, url: &str, out: &mut dyn Write) -> Result<(), InstallError>;
}
pub struct HttpTransport {
    client: reqwest::blocking::Client,
}
impl HttpTransport {
    pub fn new() -> Result<Self, InstallError> {
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(300))
            .user_agent("currspice-mod-installer/0.1")
            .build()
            .map_err(|e| InstallError::Network(e.to_string()))?;
        Ok(Self { client })
    }
}
impl Transport for HttpTransport {
    fn fetch(&mut self, url: &str, out: &mut dyn Write) -> Result<(), InstallError> {
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| InstallError::Network(format!("Could not download {url}: {e}")))?;
        let mut buf = [0; 65536];
        loop {
            let n = response
                .read(&mut buf)
                .map_err(|e| InstallError::Network(format!("Interrupted download {url}: {e}")))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
        }
        Ok(())
    }
}
/// Structural/CRC check for legacy unpinned jars; not authenticity verification.
pub fn valid_jar(path: &Path) -> bool {
    let check = || -> Result<(), Box<dyn std::error::Error>> {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
        if archive.is_empty() {
            return Err("Empty jar archive".into());
        }
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            std::io::copy(&mut entry, &mut std::io::sink())?;
        }
        Ok(())
    };
    check().is_ok()
}

pub fn file_sha256(path: &Path) -> Result<String, InstallError> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
