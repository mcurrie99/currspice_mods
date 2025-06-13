// External Imports
use std::process;
use std::error::Error;

// This file is going to handle the java installation process if it is needed as handling the
// OS system types will be annoying and having this in one location to run will make this 
// significatnly easier (just abstract this annoying stuff into oblivion and hope it doenst fail)


#[cfg(target_os = "windows")]
pub fn install_java() -> Result<(), Box<dyn Error>> {
     // Confirm that winget itself is present (ships with modern Windows).
    let winget_found = process::Command::new("cmd")
        .args(["/C", "where winget"])
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()?
        .success();

    if !winget_found {
        return Err(String::from("winget is not installed - cannot install Java automatically").into());
    }

    // 3️⃣ Kick off a silent Temurin 17+ JDK install.
    //    -e = exact ID match
    //    --accept-* = skip the interactive license prompts
    println!("⬇ Installing Eclipse Temurin 17 JDK via winget …");
    let status = process::Command::new("cmd")
        .args([
            "/C",
            "winget",
            "install",
            "--id",
            "EclipseAdoptium.Temurin.17.JDK",
            "-e",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .status()?;

    if status.success() {
        println!("✅ Java installed successfully.");
        Ok(())
    } else {
        Err(format!("Java installation failed (winget exit code {status})").into())
    }
}


#[cfg(target_os = "macos")]
pub fn install_java() -> Result<(), Box<dyn Error>> {
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn install_java() -> Result<(), Box<dyn Error>> {
  let os_release = fs::read_to_string("/etc/os-release")?;
  let id = os_release
    .lines()
    .find(|l| l.starts_with("ID="))
    .and_then(|l| l.split('=').nth(1))
    .unwrap_or("")
    .trim_matches('"');

  match id {
      "debian" | "ubuntu" | "raspbian" => {
          run(&["apt", "update"])?;
          run(&["apt", "install", "-y", "openjdk-17-jdk"])?;
      }
      "fedora" | "rhel" | "centos" => {
          run(&["dnf", "install", "-y", "java-17-openjdk"])?;
      }
      "arch" | "manjaro" => {
          run(&["pacman", "-Sy", "--noconfirm", "jdk17-openjdk"])?;
      }
      _ => {
          eprintln!("Unsupported distro: {id}. Please install Java manually.");
      }
    }

    Ok(())
}


#[allow(dead_code)]
pub fn check_java_install() -> bool {
    // Runs command to determine if java is installed
    // Command::new(JAVA_CHECK_CMD)
    // .arg("java")
    // .output()
    // .map(|output| output.status.success())
    // .unwrap_or(false)

    // New way of checking for java
    process::Command::new("java")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    }

fn run(cmd: &[&str]) -> anyhow::Result<()> {
    let status = process::Command::new(cmd[0]).args(&cmd[1..]).status()?;
    if !status.success() {
        anyhow::bail!("command {:?} failed", cmd);
    }
    Ok(())
}