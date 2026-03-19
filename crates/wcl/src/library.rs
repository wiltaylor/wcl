use std::io;
use std::path::PathBuf;

use crate::FunctionSignature;

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

/// Install a library file into the user library directory.
///
/// Returns the path where the file was written.
pub fn install_library(name: &str, content: &str) -> io::Result<PathBuf> {
    let dir = user_library_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(name);
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Remove a library file from the user library directory.
pub fn uninstall_library(name: &str) -> io::Result<()> {
    let path = user_library_dir().join(name);
    std::fs::remove_file(path)
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

/// A function stub for generating `declare` statements in library files.
pub struct FunctionStub {
    pub name: String,
    pub params: Vec<(String, String)>, // (name, type)
    pub return_type: Option<String>,
    pub doc: Option<String>,
}

impl FunctionStub {
    /// Generate a WCL `declare` statement.
    pub fn to_wcl(&self) -> String {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|(name, ty)| format!("{}: {}", name, ty))
            .collect();
        let mut out = format!("declare {}({})", self.name, params.join(", "));
        if let Some(ref rt) = self.return_type {
            out.push_str(&format!(" -> {}", rt));
        }
        out.push('\n');
        out
    }

    /// Convert this stub to a `FunctionSignature` for registry integration.
    pub fn to_signature(&self) -> FunctionSignature {
        FunctionSignature {
            name: self.name.clone(),
            params: self
                .params
                .iter()
                .map(|(name, ty)| format!("{}: {}", name, ty))
                .collect(),
            return_type: self.return_type.clone().unwrap_or_else(|| "any".into()),
            doc: self.doc.clone().unwrap_or_default(),
        }
    }
}

/// Builder for constructing a WCL library file from schemas and function stubs.
pub struct LibraryBuilder {
    name: String,
    parts: Vec<String>,
}

impl LibraryBuilder {
    pub fn new(name: &str) -> Self {
        LibraryBuilder {
            name: name.to_string(),
            parts: Vec::new(),
        }
    }

    /// Add raw WCL schema text to the library.
    pub fn add_schema_text(&mut self, schema: &str) -> &mut Self {
        self.parts.push(schema.to_string());
        self
    }

    /// Add a function stub (`declare` statement) to the library.
    pub fn add_function_stub(&mut self, stub: FunctionStub) -> &mut Self {
        self.parts.push(stub.to_wcl());
        self
    }

    /// Build the library content as a WCL string.
    pub fn build(&self) -> String {
        self.parts.join("\n")
    }

    /// Write the library to the user library directory.
    pub fn install(&self) -> io::Result<PathBuf> {
        let filename = format!("{}.wcl", self.name);
        install_library(&filename, &self.build())
    }
}
