use clap::{Parser, Subcommand};

mod commands;
mod storage;
mod tui;
mod types;

use anyhow::Result;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

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
    }
}