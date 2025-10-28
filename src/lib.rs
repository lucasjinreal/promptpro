//! PromptPro - A prompt versioning and management library
//!
//! This library provides functionality to manage text prompts with versioning, tagging,
//! and diff capabilities. It can be used as a standalone CLI tool or as a library
//! integrated into other Rust projects.

pub mod api;
mod commands;
mod storage;
mod tui;
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

// Function to run CLI from arguments
pub fn run_cli_from_args(args: Vec<String>) -> anyhow::Result<()> {
    use clap::Parser;
    
    // Define the CLI struct here to avoid duplicate definitions
    #[derive(Parser)]
    #[command(author, version, about, long_about = None)]
    struct Cli {
        #[command(subcommand)]
        command: Commands,
    }

    #[derive(clap::Subcommand)]
    enum Commands {
        /// Initialize a new prompt vault
        Init {
            /// Path to the vault directory (default: ~/promptpro/default_vault)
            #[arg(long)]
            path: Option<String>,
        },
        /// Add a new prompt
        Add {
            /// Content of the prompt
            content: String,
        },
        /// Update an existing prompt
        Update {
            /// Key of the prompt to update
            key: String,
            /// New content of the prompt
            content: String,
            /// Optional message for the update
            #[arg(short, long)]
            message: Option<String>,
        },
        /// Get a prompt by key and selector
        Get {
            /// Key of the prompt
            key: String,
            /// Selector (version, tag, latest)
            selector: Option<String>,
            /// Output to file instead of stdout
            #[arg(short, long)]
            output: Option<String>,
        },
        /// Show history of a prompt
        History {
            /// Key of the prompt
            key: String,
        },
        /// Tag a specific version of a prompt
        Tag {
            /// Key of the prompt
            key: String,
            /// Tag name
            tag: String,
            /// Version number (optional, defaults to latest)
            version: Option<u64>,
        },
        /// Promote a tag to the latest version
        Promote {
            /// Key of the prompt
            key: String,
            /// Tag name to promote
            tag: String,
        },
        /// Open TUI editor
        Tui,
        /// Edit a prompt in TUI mode
        Edit {
            /// Key of the prompt to edit
            key: String,
        },
        /// Dump the vault to a binary file
        Dump {
            /// Output file path for the dump
            output: String,
            /// Password to encrypt the dump (optional)
            #[arg(long)]
            password: Option<String>,
        },
        /// Restore/Resume the vault from a binary file
        Resume {
            /// Input file path to restore from
            input: String,
            /// Password to decrypt the dump (optional)
            #[arg(long)]
            password: Option<String>,
        },
        /// Delete a prompt by key
        Delete {
            /// Key of the prompt to delete
            key: String,
        },
    }
    
    // Skip the first argument since it's typically the program name
    let cli_args = if !args.is_empty() {
        args
    } else {
        vec!["promptpro".to_string()] // Default to showing help if no args
    };
    
    // Parse the arguments using clap
    let cli = Cli::try_parse_from(cli_args)?;
    
    // Execute the command based on the parsed arguments
    tokio::runtime::Runtime::new()?.block_on(async {
        match cli.command {
            Commands::Init { path } => commands::init(path).await,
            Commands::Add { content } => commands::add(content).await,
            Commands::Update { key, content, message } => commands::update(key, content, message).await,
            Commands::Get { key, selector, output } => commands::get(key, selector, output).await,
            Commands::History { key } => commands::history(key).await,
            Commands::Tag { key, tag, version } => commands::tag(key, tag, version).await,
            Commands::Promote { key, tag } => commands::promote(key, tag).await,
            Commands::Tui => commands::tui().await,
            Commands::Edit { key } => commands::edit(key).await,
            Commands::Dump { output, password } => commands::dump(output, password).await,
            Commands::Resume { input, password } => commands::resume(input, password).await,
            Commands::Delete { key } => commands::delete(key).await,
        }
    })
}

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
