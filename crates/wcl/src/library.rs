use std::io;
use std::path::PathBuf;

/// Return the user library directory.
///
/// Linux/macOS: `$XDG_DATA_HOME/wcl/lib/` (default: `~/.local/share/wcl/lib/`)
/// Windows:     `%APPDATA%\wcl\lib\`
pub fn user_library_dir() -> PathBuf {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(data_home).join("wcl/lib")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local/share/wcl/lib")
    } else {
        PathBuf::from(".wcl/lib")
    }
}

/// Return the system library directory.
///
/// Linux:   `/usr/share/wcl/lib/`
/// macOS:   `/usr/local/share/wcl/lib/`
/// Windows: `%PROGRAMDATA%\wcl\lib\`
pub fn system_library_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/usr/local/share/wcl/lib")
    }
    #[cfg(not(target_os = "macos"))]
    {
        PathBuf::from("/usr/share/wcl/lib")
    }
}

/// List all `.wcl` files in the user library directory.
pub fn list_libraries() -> io::Result<Vec<PathBuf>> {
    let dir = user_library_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut libs = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("wcl") {
            libs.push(path);
        }
    }
    libs.sort();
    Ok(libs)
}
