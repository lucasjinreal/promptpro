//! PromptPro - A prompt versioning and management library
//!
//! This library provides functionality to manage text prompts with versioning, tagging,
//! and diff capabilities. It can be used as a standalone CLI tool or as a library
//! integrated into other Rust projects.

pub mod api;
mod storage;
mod types;
mod utils;

#[cfg(feature = "python")]
mod sync_api;
#[cfg(feature = "python")]
mod python_bindings;

pub use storage::PromptVault;
pub use types::{VersionMeta, VersionSelector};
pub use utils::default_vault_path;

#[cfg(feature = "python")]
pub use sync_api::SyncPromptManager;

use anyhow::Result;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_library_api() -> Result<()> {
        use tempfile::tempdir;

        let dir = tempdir()?;
        let vault = PromptVault::open(dir.path())?;

        // Test adding a prompt
        vault.add("greet", "hello world")?;

        // Test getting the prompt
        let text = vault.get("greet", VersionSelector::Latest)?;
        assert_eq!(text, "hello world");

        // Test updating the prompt
        vault.update("greet", "hi there", Some("test update".to_string()))?;
        let text = vault.get("greet", VersionSelector::Latest)?;
        assert_eq!(text, "hi there");

        // Test tagging
        vault.tag("greet", "stable", 1)?; // Tag version 1 as stable
        let text = vault.get("greet", VersionSelector::Tag("stable"))?;
        assert_eq!(text, "hello world");

        // Test history
        let history = vault.history("greet")?;
        assert_eq!(history.len(), 2);

        Ok(())
    }
}
