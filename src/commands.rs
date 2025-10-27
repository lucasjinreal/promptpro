use crate::storage::PromptVault;
use crate::types::VersionSelector;
use anyhow::Result;
use std::io::{self, Write};

/// Initialize a new prompt vault
pub async fn init(path: Option<String>) -> Result<()> {
    let vault_path = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => promptpro::default_vault_path()?,
    };

    std::fs::create_dir_all(&vault_path)?;
    let _vault = PromptVault::open(&vault_path)?;
    
    println!("Initialized prompt vault at: {:?}", vault_path);
    Ok(())
}

/// Add a new prompt
pub async fn add(content: String) -> Result<()> {
    let vault = PromptVault::open_default()?;

    print!("Enter key name: ");
    io::stdout().flush()?;
    
    let mut key = String::new();
    io::stdin().read_line(&mut key)?;
    key = key.trim().to_string();

    vault.add(&key, &content)?;
    
    println!("[+] Stored prompt under key: {}", key);
    println!("    version: 1 (snapshot)");
    println!("    vault: {:?}", promptpro::default_vault_path()?);

    Ok(())
}

/// Update an existing prompt
pub async fn update(key: String, content: String, message: Option<String>) -> Result<()> {
    let vault = PromptVault::open_default()?;
    
    match vault.update(&key, &content, message) {
        Ok(()) => {
            println!("[+] Updated prompt: {}", key);
            
            // Get the new latest version
            if let Ok(Some(version)) = get_latest_version_number(&vault, &key) {
                println!("    version: {} (updated)", version);
                println!("    'dev' tag automatically updated to latest version");
                println!("    vault: {:?}", promptpro::default_vault_path()?);
            }
        },
        Err(e) => {
            eprintln!("Error updating prompt: {}", e);
        }
    }

    Ok(())
}

/// Get a prompt by key and selector
pub async fn get(key: String, selector: Option<String>, output: Option<String>) -> Result<()> {
    let vault = PromptVault::open_default()?;
    
    let sel = match selector {
        Some(s) => {
            // Try to parse as version number first
            if let Ok(version) = s.parse::<u64>() {
                VersionSelector::Version(version)
            } else if s == "latest" {
                VersionSelector::Latest
            } else {
                // Assume it's a tag - use a temporary string and make it static for this use case
                // This is a simplified implementation, in a real one we'd handle lifetimes differently
                VersionSelector::Tag(Box::leak(s.into_boxed_str()))
            }
        },
        None => VersionSelector::Latest,
    };

    let content = vault.get(&key, sel)?;
    
    match output {
        Some(file_path) => {
            std::fs::write(file_path, &content)?;
            println!("Prompt content saved to file");
        },
        None => {
            println!("{}", content);
        }
    }

    Ok(())
}

/// Show history of a prompt
pub async fn history(key: String) -> Result<()> {
    let vault = PromptVault::open_default()?;
    
    let versions = vault.history(&key)?;
    
    if versions.is_empty() {
        println!("No versions found for key: {}", key);
        return Ok(());
    }

    println!("History for key: {}", key);
    println!("{:<5} {:<20} {:<15} {:<30} {}", "Ver", "Timestamp", "Tags", "Message", "Content Preview");
    println!("{}", "-".repeat(120));

    for version in versions {
        let timestamp = version.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        let tags_str = version.tags.join(",");
        let message = version.message.unwrap_or_default();
        
        // Get content for preview
        let content_preview = match vault.get(&key, VersionSelector::Version(version.version)) {
            Ok(content) => {
                // Take first 40 characters or until first newline
                let preview = content.lines().next().unwrap_or(&content);
                let preview = if preview.len() > 40 {
                    &preview[..40]
                } else {
                    preview
                };
                format!("{}", preview)
            },
            Err(_) => "Content unavailable".to_string(),
        };
        
        println!(
            "{:<5} {:<20} {:<15} {:<30} {}", 
            version.version, 
            timestamp, 
            tags_str, 
            message,
            content_preview
        );
    }

    Ok(())
}

/// Tag a specific version of a prompt
pub async fn tag(key: String, tag: String, version: Option<u64>) -> Result<()> {
    let vault = PromptVault::open_default()?;
    
    let version_to_tag = match version {
        Some(v) => v,
        None => {
            // Use latest version if no version specified
            match get_latest_version_number(&vault, &key)? {
                Some(v) => v,
                None => return Err(anyhow::anyhow!("No versions found for key '{}'", key)),
            }
        }
    };

    vault.tag(&key, &tag, version_to_tag)?;
    println!("Tagged version {} of '{}' as '{}'", version_to_tag, key, tag);

    Ok(())
}

/// Promote a tag to the latest version
pub async fn promote(key: String, tag: String) -> Result<()> {
    let vault = PromptVault::open_default()?;
    
    vault.promote(&key, &tag)?;
    println!("Promoted tag '{}' of '{}' to latest version", tag, key);

    Ok(())
}

/// Open TUI editor
pub async fn tui() -> Result<()> {
    println!("Opening TUI editor...");
    crate::tui::run().await
}

/// Edit a prompt in TUI mode
pub async fn edit(key: String) -> Result<()> {
    println!("Opening TUI editor for key: {}", key);
    crate::tui::run_with_key(key).await
}

/// Dump the vault to a binary file
pub async fn dump(output: String, password: Option<String>) -> Result<()> {
    let vault = PromptVault::open_default()?;
    let password_ref = password.as_deref();
    
    match vault.dump(&output, password_ref) {
        Ok(()) => {
            println!("Vault dumped successfully to: {}", output);
            if password.is_some() {
                println!("Vault is encrypted with provided password");
            } else {
                println!("Vault is unencrypted");
            }
        },
        Err(e) => {
            eprintln!("Error dumping vault: {}", e);
        }
    }
    
    Ok(())
}

/// Restore/Resume the vault from a binary file
pub async fn resume(input: String, password: Option<String>) -> Result<()> {
    use std::fs;

    
    let password_ref = password.as_deref();
    
    // Create a temporary vault from the dump file
    match PromptVault::restore(&input, password_ref) {
        Ok(restored_vault) => {
            // Get the default vault path
            let default_dir = std::env::var("HOME")?;
            let default_vault_path = std::path::PathBuf::from(default_dir).join(".promptpro").join("default_vault");
            
            // Ensure the parent directory exists
            if let Some(parent) = default_vault_path.parent() {
                fs::create_dir_all(parent)?;
            }
            
            // Close the restored vault to ensure files are flushed
            restored_vault.db().flush()?;
            
            // Since sled creates multiple files, we'll copy the content differently
            // Open the default vault and copy entries from the restored vault
            let target_vault = PromptVault::open(&default_vault_path)?;
            
            // Clear the target vault first to avoid conflicts
            // For sled, we'll just copy entries over which will overwrite
            // Copy all entries from the restored vault to the target vault
            for result in restored_vault.db().iter() {
                let (key, value) = result?;
                target_vault.db().insert(key, value)?;
            }
            
            // Flush the target vault to ensure data is written
            target_vault.db().flush()?;
            
            println!("Vault restored successfully from: {}", input);
            if password.is_some() {
                println!("Vault was encrypted with provided password");
            } else {
                println!("Vault was unencrypted");
            }
            
            // Count number of entries in the target vault as validation
            let mut count = 0;
            for result in target_vault.db().iter() {
                if result.is_ok() {
                    count += 1;
                }
            }
            println!("Restored {} entries to the default vault", count);
        },
        Err(e) => {
            eprintln!("Error resuming vault: {}", e);
        }
    }
    
    Ok(())
}

/// Helper function to get the latest version number for a key
fn get_latest_version_number(vault: &PromptVault, key: &str) -> Result<Option<u64>> {
    let mut versions = Vec::new();
    let history = vault.history(key)?;
    
    for version in history {
        versions.push(version.version);
    }

    if versions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(*versions.iter().max().unwrap()))
    }
}