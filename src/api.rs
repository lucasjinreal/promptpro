use anyhow::Result;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{PromptVault, VersionSelector};

/// Default global prompt manager (singleton)
pub struct DefaultPromptManager {
    vault: Arc<RwLock<PromptVault>>,
}

/// Static global instance of the default manager
static DEFAULT_MANAGER: Lazy<DefaultPromptManager> = Lazy::new(|| {
    let vault = PromptVault::open_default().expect("Failed to open PromptPro default vault");
    DefaultPromptManager {
        vault: Arc::new(RwLock::new(vault)),
    }
});

impl DefaultPromptManager {
    /// Get a reference to the global singleton
    pub fn get() -> &'static Self {
        &DEFAULT_MANAGER
    }

    /// Add a prompt (creates if missing)
    pub async fn add(&self, key: &str, content: &str) -> Result<()> {
        let vault = self.vault.write().await;
        vault.add(key, content)?;
        Ok(())
    }

    /// Update a prompt version
    pub async fn update(&self, key: &str, content: &str, message: Option<&str>) -> Result<()> {
        let vault = self.vault.write().await;
        vault.update(key, content, message.map(|s| s.to_string()))?;
        Ok(())
    }

    /// Tag a version (e.g. stable/release/dev)
    pub async fn tag(&self, key: &str, tag: &str, version: u64) -> Result<()> {
        let vault = self.vault.write().await;
        vault.tag(key, tag, version)?;
        Ok(())
    }

    /// Retrieve a prompt by version/tag
    pub async fn get_prompt(&self, key: &str, selector: VersionSelector<'_>) -> Result<String> {
        let vault = self.vault.read().await;
        Ok(vault.get(key, selector)?)
    }

    /// Retrieve latest prompt
    pub async fn latest(&self, key: &str) -> Result<String> {
        self.get_prompt(key, VersionSelector::Latest).await
    }

    /// List history of versions
    pub async fn history(&self, key: &str) -> Result<()> {
        let vault = self.vault.read().await;
        for v in vault.history(key)? {
            println!(
                "Version {} | {} | {:?}",
                v.version,
                v.timestamp.format("%Y-%m-%d %H:%M"),
                v.tags
            );
        }
        Ok(())
    }

    /// Export (backup)
    pub async fn backup(&self, path: &str, password: Option<&str>) -> Result<()> {
        let vault = self.vault.read().await;
        vault.dump(path, password.map(|p| p))?;
        Ok(())
    }
}
