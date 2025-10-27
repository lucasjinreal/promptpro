use anyhow::Result;
use std::path::PathBuf;

/// Get the default vault path: ~/.promptpro/default_vault
pub fn default_vault_path() -> Result<PathBuf> {
    let home_dir = std::env::var("HOME")?;
    Ok(PathBuf::from(home_dir).join(".promptpro").join("default_vault"))
}