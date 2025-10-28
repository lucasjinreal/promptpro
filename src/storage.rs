use crate::types::{VersionMeta, VersionSelector};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{Context, Result};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffOp, TextDiff};
use std::{collections::HashMap, fs, path::PathBuf};
use std::{io::Read, path::Path};

/// The main storage backend for prompt versions
#[derive(Clone)]
pub struct PromptVault {
    db: sled::Db,
}

impl PromptVault {
    pub fn restore_or_default(input_path: &str, password: Option<&str>) -> Result<Self> {
        let input = Path::new(input_path);

        if input.exists() {
            println!("🔄 Found vault file at '{}', restoring...", input.display());
            Self::restore(input_path, password)
        } else {
            println!(
                "⚠️ Vault file '{}' not found — opening default vault instead.",
                input.display()
            );
            Self::open_default().map_err(|e| anyhow::anyhow!("Failed to open default vault: {}", e))
        }
    }

    pub fn open_or_default<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();

        // Try opening user-specified path
        match Self::open(path_ref) {
            Ok(vault) => Ok(vault),
            Err(e) => {
                eprintln!(
                    "⚠️ Failed to open vault at {:?}: {}. Falling back to default vault...",
                    path_ref, e
                );
                // Try default
                Self::open_default().with_context(|| {
                    format!(
                        "Failed to open both specified vault {:?} and default vault",
                        path_ref
                    )
                })
            }
        }
    }

    /// Open a prompt vault at the specified path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(PromptVault { db })
    }

    /// Open the default prompt vault
    pub fn open_default() -> Result<Self> {
        let home_dir = std::env::var("HOME")?;
        let path = std::path::PathBuf::from(home_dir)
            .join(".promptpro")
            .join("default_vault");
        std::fs::create_dir_all(&path)?;
        Self::open(path)
    }

    /// Add a new prompt with the given key and content
    pub fn add(&self, key: &str, content: &str) -> Result<()> {
        // Check if the key already exists
        if self.get_latest_version_number(key)?.is_some() {
            return Err(anyhow::anyhow!("Prompt with key '{}' already exists", key));
        }

        // Create initial version (version 1) - always a snapshot
        let version_meta = VersionMeta::new(key.to_string(), 1, content, None, None);

        self.store_version(&version_meta, content, None)?;
        Ok(())
    }

    /// Update an existing prompt with new content
    pub fn update(&self, key: &str, content: &str, message: Option<String>) -> Result<()> {
        // Get the latest version to use as parent
        let latest_version = self.get_latest_version_number(key)?;
        let parent_version = match latest_version {
            Some(v) => v,
            None => return Err(anyhow::anyhow!("Prompt with key '{}' does not exist", key)),
        };

        // Get the current content to check if there are changes
        let current_content = self.get_content(&key, &VersionSelector::Version(parent_version))?;
        if current_content == content {
            return Err(anyhow::anyhow!("No changes detected in content"));
        }

        // Always create a new version (snapshot) for now
        // In a more complex implementation, we might decide to use diffs
        let new_version = parent_version + 1;
        let snapshot = true; // Always store as snapshot for simplicity and reliability
        let diff_content = None; // We're using snapshots

        // Create new version metadata
        let mut version_meta = VersionMeta::new(
            key.to_string(),
            new_version,
            content,
            Some(parent_version),
            message,
        );
        version_meta.snapshot = snapshot;

        self.store_version(&version_meta, content, diff_content)?;

        // Always promote the 'dev' tag to the new latest version
        // This ensures dev always points to the most recent version
        let _ = self.tag(key, "dev", new_version); // Ignore errors

        Ok(())
    }

    /// Get prompt content by key and selector
    pub fn get(&self, key: &str, selector: VersionSelector) -> Result<String> {
        let version_number = match selector {
            VersionSelector::Latest => self
                .get_latest_version_number(key)?
                .ok_or_else(|| anyhow::anyhow!("No versions found for key '{}'", key))?,
            VersionSelector::Version(v) => v,
            VersionSelector::Tag(tag) => self
                .get_version_by_tag(key, tag)?
                .ok_or_else(|| anyhow::anyhow!("Tag '{}' not found for key '{}'", tag, key))?,
            VersionSelector::Time(time) => {
                self.get_version_by_time(key, time)?.ok_or_else(|| {
                    anyhow::anyhow!("No version found for key '{}' at time {}", key, time)
                })?
            }
        };

        self.get_content(key, &VersionSelector::Version(version_number))
    }

    /// Get history of all versions for a key
    pub fn history(&self, key: &str) -> Result<Vec<VersionMeta>> {
        // Get all versions for the key
        let mut versions = Vec::new();
        let prefix = format!("version:{}:", key);

        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (_key, value) = result?;
            let version_meta: VersionMeta = bincode::deserialize(&value)?;
            versions.push(version_meta);
        }

        // Sort by version number
        versions.sort_by_key(|v| v.version);
        Ok(versions)
    }

    /// Tag a specific version
    pub fn tag(&self, key: &str, tag: &str, version: u64) -> Result<()> {
        // Check if the version exists
        let version_key = format!("version:{}:{}", key, version);
        if self.db.get(version_key.as_bytes())?.is_none() {
            return Err(anyhow::anyhow!(
                "Version {} does not exist for key '{}'",
                version,
                key
            ));
        }

        // For 'dev' tag, we always enforce it points to the latest version
        if tag == "dev" {
            let latest_version = self
                .get_latest_version_number(key)?
                .ok_or_else(|| anyhow::anyhow!("No versions found for key '{}'", key))?;

            // If user is trying to set dev to an older version, deny it
            if version != latest_version {
                return Err(anyhow::anyhow!(
                    "'dev' tag can only be set to the latest version (v{})",
                    latest_version
                ));
            }
        }

        // First, remove the tag from any other version that currently has it
        if let Ok(Some(old_version)) = self.get_version_by_tag(key, tag) {
            if old_version != version {
                // Remove the tag from the old version's metadata
                let mut old_version_meta =
                    self.get_version_meta(key, old_version)?.ok_or_else(|| {
                        anyhow::anyhow!("Version {} not found for key '{}'", old_version, key)
                    })?;

                // Remove the tag from the old version's tag list
                old_version_meta.tags.retain(|t| t != tag);
                self.update_version_meta(&old_version_meta)?;
            }
        }

        // Create/update the tag entry to point to the new version
        let tag_key = format!("tag:{}:{}", key, tag);
        self.db.insert(tag_key.as_bytes(), &version.to_le_bytes())?;

        // Update the new version's metadata to include the tag
        let mut version_meta = self
            .get_version_meta(key, version)?
            .ok_or_else(|| anyhow::anyhow!("Version {} not found for key '{}'", version, key))?;

        if !version_meta.tags.contains(&tag.to_string()) {
            version_meta.tags.push(tag.to_string());
            self.update_version_meta(&version_meta)?;
        }

        Ok(())
    }

    /// Promote a tag to point to the latest version
    pub fn promote(&self, key: &str, tag: &str) -> Result<()> {
        // For 'dev' tag, we always promote to latest, but it's already handled in update()
        // For 'stable' and 'release', we allow manual promotion to latest
        let latest_version = self
            .get_latest_version_number(key)?
            .ok_or_else(|| anyhow::anyhow!("No versions found for key '{}'", key))?;

        self.tag(key, tag, latest_version)
    }

    /// Get the latest version number for a key
    pub fn get_latest_version_number(&self, key: &str) -> Result<Option<u64>> {
        let mut versions = Vec::new();
        let prefix = format!("version:{}:", key);

        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (_key, value) = result?;
            let version_meta: VersionMeta = bincode::deserialize(&value)?;
            versions.push(version_meta.version);
        }

        if versions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(*versions.iter().max().unwrap()))
        }
    }

    /// Get version number by tag
    fn get_version_by_tag(&self, key: &str, tag: &str) -> Result<Option<u64>> {
        let tag_key = format!("tag:{}:{}", key, tag);
        if let Some(value) = self.db.get(tag_key.as_bytes())? {
            let version_bytes: [u8; 8] = value
                .as_ref()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Failed to read version from tag"))?;
            let version = u64::from_le_bytes(version_bytes);
            Ok(Some(version))
        } else {
            Ok(None)
        }
    }

    /// Get version number by timestamp
    fn get_version_by_time(
        &self,
        key: &str,
        time: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<u64>> {
        let mut versions = Vec::new();
        let prefix = format!("version:{}:", key);

        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (_key, value) = result?;
            let version_meta: VersionMeta = bincode::deserialize(&value)?;
            versions.push(version_meta);
        }

        // Find the version with the timestamp closest to but not exceeding the given time
        versions.retain(|v| v.timestamp <= time);
        versions.sort_by_key(|v| v.version);

        Ok(versions.last().map(|v| v.version))
    }

    /// Get the content of a specific version
    fn get_content(&self, key: &str, selector: &VersionSelector) -> Result<String> {
        let version = match selector {
            VersionSelector::Version(v) => *v,
            _ => return Err(anyhow::anyhow!("Invalid selector for content retrieval")),
        };

        // Get the version metadata to check if it's a snapshot or diff
        let version_meta = self
            .get_version_meta(key, version)?
            .ok_or_else(|| anyhow::anyhow!("Version {} not found for key '{}'", version, key))?;

        if version_meta.snapshot {
            // For snapshots, content is stored directly
            let content_key = format!("content:{}:{}", key, version);
            if let Some(content_bytes) = self.db.get(content_key.as_bytes())? {
                Ok(String::from_utf8(content_bytes.to_vec())?)
            } else {
                Err(anyhow::anyhow!(
                    "Content not found for key '{}', version {}, make sure key were added.",
                    key,
                    version
                ))
            }
        } else {
            // For diffs, we need to reconstruct from parent
            let diff_key = format!("diff:{}:{}", key, version);
            if let Some(diff_bytes) = self.db.get(diff_key.as_bytes())? {
                let diff_str = String::from_utf8(diff_bytes.to_vec())?;

                // Get parent content
                let parent_version = version_meta.parent.ok_or_else(|| {
                    anyhow::anyhow!("Diff version {} missing parent reference", version)
                })?;

                let parent_content =
                    self.get_content(key, &VersionSelector::Version(parent_version))?;

                // Apply the diff to get current content
                let current_content = apply_diff(&parent_content, &diff_str)?;
                Ok(current_content)
            } else {
                Err(anyhow::anyhow!(
                    "Diff not found for key '{}', version {}",
                    key,
                    version
                ))
            }
        }
    }

    /// Store a version with its content
    fn store_version(
        &self,
        version_meta: &VersionMeta,
        content: &str,
        _diff_content: Option<String>,
    ) -> Result<()> {
        // Store the version metadata
        let version_key = format!("version:{}:{}", version_meta.key, version_meta.version);
        let meta_bytes = bincode::serialize(version_meta)?;
        self.db.insert(version_key.as_bytes(), meta_bytes)?;

        // Always store full content for snapshots (now all versions are snapshots)
        let content_key = format!("content:{}:{}", version_meta.key, version_meta.version);
        self.db.insert(content_key.as_bytes(), content.as_bytes())?;

        Ok(())
    }

    /// Get version metadata
    fn get_version_meta(&self, key: &str, version: u64) -> Result<Option<VersionMeta>> {
        let version_key = format!("version:{}:{}", key, version);

        if let Some(value) = self.db.get(version_key.as_bytes())? {
            let version_meta: VersionMeta = bincode::deserialize(&value)?;
            Ok(Some(version_meta))
        } else {
            Ok(None)
        }
    }

    /// Update version metadata (used when adding tags)
    fn update_version_meta(&self, version_meta: &VersionMeta) -> Result<()> {
        let version_key = format!("version:{}:{}", version_meta.key, version_meta.version);
        let meta_bytes = bincode::serialize(version_meta)?;
        self.db.insert(version_key.as_bytes(), meta_bytes)?;
        Ok(())
    }

    /// Get access to the underlying database (for TUI usage)
    pub fn db(&self) -> &sled::Db {
        &self.db
    }

    /// Delete a prompt key and all its versions
    pub fn delete_prompt_key(&self, key: &str) -> Result<()> {
        // Get all versions for this key to clean up related data
        let versions = self.history(key)?;
        
        // Delete all version entries and related content/diff data
        for version in &versions {
            let version_key = format!("version:{}:{}", key, version.version);
            self.db.remove(version_key.as_bytes())?;
            
            // Delete content for this version
            let content_key = format!("content:{}:{}", key, version.version);
            self.db.remove(content_key.as_bytes())?;
            
            // Delete diff if it exists (for future compatibility)
            let diff_key = format!("diff:{}:{}", key, version.version);
            self.db.remove(diff_key.as_bytes())?;
        }
        
        // Delete all tag entries for this key
        let tag_prefix = format!("tag:{}:", key);
        for result in self.db.scan_prefix(tag_prefix.as_bytes()) {
            let (tag_key, _) = result?;
            self.db.remove(tag_key)?;
        }
        
        Ok(())
    }

    /// Export the entire vault to a binary file
    pub fn dump(&self, output_path: &str, password: Option<&str>) -> Result<()> {
        use std::fs::File;
        use std::io::Write;

        // Collect all data from sled database
        let mut data = Vec::new();
        for result in self.db.iter() {
            let (key, value) = result?;
            data.push((key.to_vec(), value.to_vec()));
        }

        // Serialize the data
        let serialized_data = bincode::serialize(&data)?;

        let output_data = if let Some(password) = password {
            // Encrypt the data
            let encrypted = self.encrypt_data(&serialized_data, password)?;
            // Add a header to indicate this is encrypted
            let mut output = b"VAULT_ENC".to_vec(); // 9-byte header
            output.extend_from_slice(&encrypted);
            output
        } else {
            // Not encrypted - add header to indicate unencrypted
            let mut output = b"VAULT_RAW".to_vec(); // 9-byte header
            output.extend_from_slice(&serialized_data);
            output
        };

        // Write to file
        let mut file = File::create(output_path)?;
        file.write_all(&output_data)?;

        Ok(())
    }

    /// Import data from a binary vault file
    pub fn restore(input_path: &str, password: Option<&str>) -> Result<Self> {
        let input_path = Path::new(input_path);
        if !input_path.exists() {
            return Err(anyhow::anyhow!(
                "Vault file not found: {}",
                input_path.display()
            ));
        }

        // vault_name = filename without extension
        let vault_name = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid vault filename"))?;

        // default restore dir
        let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME env not found"))?;
        let target_path = PathBuf::from(home).join(".promptpro").join(vault_name);

        // if already exists, skip restore
        if target_path.exists() {
            println!(
                "✅ Vault '{}' already exists — skipping restore.",
                vault_name
            );
            return Self::open(&target_path);
        }

        // read full file
        let mut data = Vec::new();

        std::fs::File::open(input_path)?.read_to_end(&mut data)?;
        if data.len() < 9 {
            return Err(anyhow::anyhow!("Invalid vault file: too short"));
        }

        let header = &data[..9];
        let payload = &data[9..];

        // decrypt or raw load
        let raw = if header == b"VAULT_ENC" {
            if let Some(pwd) = password {
                Self::decrypt_data(payload, pwd)?
            } else {
                return Err(anyhow::anyhow!("Vault encrypted but no password provided"));
            }
        } else if header == b"VAULT_RAW" {
            payload.to_vec()
        } else {
            return Err(anyhow::anyhow!("Invalid vault file header"));
        };

        // deserialize data
        let entries: Vec<(Vec<u8>, Vec<u8>)> = bincode::deserialize(&raw)
            .map_err(|_| anyhow::anyhow!("Failed to deserialize vault"))?;

        // create target dir and insert
        fs::create_dir_all(&target_path)?;
        let vault = Self::open(&target_path)?;

        for (k, v) in entries {
            vault.db.insert(k, v)?;
        }
        vault.db.flush()?;

        println!(
            "✅ Restored vault '{}' → {}",
            vault_name,
            target_path.display()
        );

        Ok(vault)
    }

    /// Encrypt data with the given password
    fn encrypt_data(&self, data: &[u8], password: &str) -> Result<Vec<u8>> {
        use blake3;

        // Derive a key from the password using blake3
        let mut salt = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);

        // Derive key using blake3
        let mut key_bytes = [0u8; 32];
        let mut hasher = blake3::Hasher::new();
        hasher.update(password.as_bytes());
        hasher.update(&salt);
        let hash = hasher.finalize();
        (&mut key_bytes).copy_from_slice(&hash.as_bytes()[..32]);

        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the data
        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Combine salt + nonce + ciphertext
        let mut result = Vec::new();
        result.extend_from_slice(&salt);
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt data with the given password
    fn decrypt_data(data: &[u8], password: &str) -> Result<Vec<u8>> {
        use blake3;

        if data.len() < 44 {
            // 32 bytes salt + 12 bytes nonce + at least 1 byte of ciphertext
            return Err(anyhow::anyhow!("Encrypted data is too short"));
        }

        // Extract salt, nonce, and ciphertext
        let salt = &data[0..32];
        let nonce_bytes = &data[32..44];
        let ciphertext = &data[44..];

        // Derive key from password and salt
        let mut key_bytes = [0u8; 32];
        let mut hasher = blake3::Hasher::new();
        hasher.update(password.as_bytes());
        hasher.update(salt);
        let hash = hasher.finalize();
        (&mut key_bytes).copy_from_slice(&hash.as_bytes()[..32]);

        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(nonce_bytes);

        // Decrypt the data
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

        Ok(plaintext)
    }
}

/// Apply a diff to old content to get new content (placeholder - not used when using snapshots)
fn apply_diff(_old_content: &str, _diff_str: &str) -> Result<String> {
    // This function is not used when using snapshots only
    Ok("".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_vault_operations() -> Result<()> {
        let dir = tempdir()?;
        let vault = PromptVault::open(dir.path())?;

        // Test adding a prompt
        vault.add("test_key", "initial content")?;

        // Test getting the prompt
        let content = vault.get("test_key", VersionSelector::Latest)?;
        assert_eq!(content, "initial content");

        // Test updating the prompt
        vault.update(
            "test_key",
            "updated content",
            Some("test message".to_string()),
        )?;

        // Test getting the updated prompt
        let content = vault.get("test_key", VersionSelector::Latest)?;
        assert_eq!(content, "updated content");

        // Test history
        let history = vault.history("test_key")?;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version, 1);
        assert_eq!(history[1].version, 2);
        assert_eq!(history[1].message, Some("test message".to_string()));

        Ok(())
    }

    #[test]
    fn test_tagging() -> Result<()> {
        let dir = tempdir()?;
        let vault = PromptVault::open(dir.path())?;

        vault.add("test_key", "content v1")?;
        vault.update("test_key", "content v2", None)?;

        // Tag version 1 as "stable"
        vault.tag("test_key", "stable", 1)?;

        // Get content by tag
        let content = vault.get("test_key", VersionSelector::Tag("stable"))?;
        assert_eq!(content, "content v1");

        // Promote tag to latest
        vault.promote("test_key", "stable")?;
        let content = vault.get("test_key", VersionSelector::Tag("stable"))?;
        assert_eq!(content, "content v2");

        Ok(())
    }

    #[test]
    fn test_dev_tag_logic() -> Result<()> {
        let dir = tempdir()?;
        let vault = PromptVault::open(dir.path())?;

        // Add initial version
        vault.add("test_key", "content v1")?;

        // Update to create a second version - dev should automatically be on latest (v2)
        vault.update("test_key", "content v2", None)?;

        // Check that dev tag points to latest version (should be v2)
        let history = vault.history("test_key")?;
        assert_eq!(history.len(), 2);

        // v2 should have the dev tag
        let latest_version = history.last().unwrap();
        assert_eq!(latest_version.version, 2);
        assert!(latest_version.tags.contains(&"dev".to_string()));

        // v1 should not have the dev tag
        let first_version = &history[0];
        assert_eq!(first_version.version, 1);
        assert!(!first_version.tags.contains(&"dev".to_string()));

        // Try to manually tag v1 as dev - this should fail
        let result = vault.tag("test_key", "dev", 1);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("'dev' tag can only be set to the latest version"));

        // Now update again to create v3 - dev should automatically move to v3
        vault.update("test_key", "content v3", None)?;

        let new_history = vault.history("test_key")?;
        assert_eq!(new_history.len(), 3);

        // v3 should have the dev tag now
        let latest_version = new_history.last().unwrap();
        assert_eq!(latest_version.version, 3);
        assert!(latest_version.tags.contains(&"dev".to_string()));

        // v2 should no longer have the dev tag
        let second_version = &new_history[1]; // v2
        assert_eq!(second_version.version, 2);
        assert!(!second_version.tags.contains(&"dev".to_string()));

        Ok(())
    }

    #[test]
    fn test_dump_restore_unencrypted() -> Result<()> {
        use tempfile::tempdir;
        let source_dir = tempdir()?;
        let _target_dir = tempdir()?;

        let source_vault = PromptVault::open(source_dir.path())?;

        // Add some data to the source vault
        source_vault.add("test_key", "test content")?;
        source_vault.update(
            "test_key",
            "updated content",
            Some("test update".to_string()),
        )?;
        source_vault.tag("test_key", "stable", 1)?;

        // Check original content
        let original_content = source_vault.get("test_key", VersionSelector::Latest)?;
        assert_eq!(original_content, "updated content");

        // Dump the vault to a file
        let dump_file = source_dir.path().join("test_dump.vault");
        source_vault.dump(dump_file.to_str().unwrap(), None)?;

        // Restore to a new vault location
        let restored_vault = PromptVault::restore(dump_file.to_str().unwrap(), None)?;

        // Check that the restored data is the same
        let content = restored_vault.get("test_key", VersionSelector::Latest)?;
        assert_eq!(content, "updated content");

        // Check history
        let history = restored_vault.history("test_key")?;
        assert_eq!(history.len(), 2);
        // Check the first version (v1) - it should have the content from the first add
        assert_eq!(history[0].version, 1);
        // The message will be from the first version, which was just "initial content" for adds
        // Actually, when adding the original content, there's no message - let's just check tags
        assert!(history[0].tags.contains(&"stable".to_string()));

        Ok(())
    }

    #[test]
    fn test_dump_restore_encrypted() -> Result<()> {
        use tempfile::tempdir;
        let source_dir = tempdir()?;
        let _target_dir = tempdir()?;

        let source_vault = PromptVault::open(source_dir.path())?;

        // Add some data to the source vault
        source_vault.add("encrypted_key", "secret content")?;
        source_vault.update(
            "encrypted_key",
            "updated secret content",
            Some("update message".to_string()),
        )?;
        source_vault.tag("encrypted_key", "secret", 1)?;

        // Dump the vault to a file with password
        let dump_file = source_dir.path().join("encrypted_dump.vault");
        source_vault.dump(dump_file.to_str().unwrap(), Some("mypassword"))?;

        // Restore the vault from the file with correct password
        let restored_vault = PromptVault::restore(dump_file.to_str().unwrap(), Some("mypassword"))?;

        // Check that the restored data is the same
        let content = restored_vault.get("encrypted_key", VersionSelector::Latest)?;
        assert_eq!(content, "updated secret content");

        // Check history
        let history = restored_vault.history("encrypted_key")?;
        assert_eq!(history.len(), 2);
        assert!(history[0].tags.contains(&"secret".to_string()));

        // Try to restore with wrong password - should fail
        let result = PromptVault::restore(dump_file.to_str().unwrap(), Some("wrongpassword"));
        assert!(result.is_err());

        Ok(())
    }
}
