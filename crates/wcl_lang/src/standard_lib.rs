use std::path::PathBuf;

pub const CODECS_LIBRARY_WCL: &str = include_str!("std/codecs.wcl");

pub fn install_codecs_library(force: bool) -> Result<PathBuf, String> {
    let lib_dir = crate::library::user_library_dir();
    std::fs::create_dir_all(&lib_dir)
        .map_err(|e| format!("failed to create library dir {}: {e}", lib_dir.display()))?;
    let target = lib_dir.join("codecs.wcl");
    if target.exists() && !force {
        return Err(format!(
            "{} already exists (use --force to overwrite)",
            target.display()
        ));
    }
    std::fs::write(&target, CODECS_LIBRARY_WCL)
        .map_err(|e| format!("failed to write {}: {e}", target.display()))?;
    Ok(target)
}
