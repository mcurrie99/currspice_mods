#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();

    // Path to your .ico file (must exist)
    res.set_icon("icon.ico");

    res.compile().expect("Failed to compile resources");
}

#[cfg(not(windows))]
fn main() {
    
}
