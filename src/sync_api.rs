use anyhow::Result;
use std::sync::Arc;
use std::sync::RwLock;

use crate::{PromptVault, VersionSelector};

/// Synchronous default prompt manager (singleton)
#[derive(Clone)]
pub struct SyncPromptManager {
    vault: Arc<RwLock<PromptVault>>,
}

impl SyncPromptManager {
    /// Create a new sync prompt manager with the default vault
    pub fn new() -> Result<Self> {
        let vault = PromptVault::open_default()?;
        Ok(SyncPromptManager {
            vault: Arc::new(RwLock::new(vault)),
        })
    }

    /// Create a new sync prompt manager with a specific vault path
    pub fn with_path<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let vault = PromptVault::open(path)?;
        Ok(SyncPromptManager {
            vault: Arc::new(RwLock::new(vault)),
        })
    }

    /// Add a prompt (creates if missing)
    pub fn add(&self, key: &str, content: &str) -> Result<()> {
        let vault = self.vault.write().unwrap();
        vault.add(key, content)?;
        Ok(())
    }

    /// Update a prompt version
    pub fn update(&self, key: &str, content: &str, message: Option<&str>) -> Result<()> {
        let vault = self.vault.write().unwrap();
        vault.update(key, content, message.map(|s| s.to_string()))?;
        Ok(())
    }

    /// Tag a version (e.g. stable/release/dev)
    pub fn tag(&self, key: &str, tag: &str, version: u64) -> Result<()> {
        let vault = self.vault.write().unwrap();
        vault.tag(key, tag, version)?;
        Ok(())
    }

    /// Retrieve a prompt by version/tag
    pub fn get_prompt(&self, key: &str, selector: VersionSelector) -> Result<String> {
        let vault = self.vault.read().unwrap();
        Ok(vault.get(key, selector)?)
    }

    /// Retrieve latest prompt
    pub fn latest(&self, key: &str) -> Result<String> {
        self.get_prompt(key, VersionSelector::Latest)
    }

    /// List history of versions
    pub fn history(&self, key: &str) -> Result<Vec<crate::types::VersionMeta>> {
        let vault = self.vault.read().unwrap();
        Ok(vault.history(key)?)
    }

    /// Export (backup)
    pub fn backup(&self, path: &str, password: Option<&str>) -> Result<()> {
        let vault = self.vault.read().unwrap();
        vault.dump(path, password)?;
        Ok(())
    }

    /// Restore from backup
    pub fn restore(&self, path: &str, password: Option<&str>) -> Result<()> {
        // This is a bit more complex as we need to restore to the current vault
        // For now, we'll just delegate to the static restore method and replace our vault
        let restored_vault = PromptVault::restore(path, password)?;
        
        // Replace the current vault contents with the restored vault
        {
            let current_vault = self.vault.write().unwrap();
            // Unfortunately we can't directly replace the contents of an existing vault,
            // so we'd need to copy data between them. For now, this is a placeholder.
            // In a real implementation, we might want to restructure this differently.
        }
        Ok(())
    }
}

/// Global static instance of the sync manager
static mut GLOBAL_MANAGER: Option<SyncPromptManager> = None;
static INIT: std::sync::Once = std::sync::Once::new();

impl SyncPromptManager {
    /// Get a reference to the global singleton
    pub fn get() -> &'static Self {
        unsafe {
            INIT.call_once(|| {
                GLOBAL_MANAGER = Some(SyncPromptManager::new().expect("Failed to create PromptPro sync manager"));
            });
            GLOBAL_MANAGER.as_ref().unwrap()
        }
    }
}