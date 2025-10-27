use anyhow::Result;
use promptpro::api::DefaultPromptManager;
use promptpro::VersionSelector;

#[tokio::main]
async fn main() -> Result<()> {
    let pm = DefaultPromptManager::get();

    pm.add("summarization", "Summarize the following text...")
        .await?;
    pm.update(
        "summarization",
        "Provide a concise summary of the text, keeping context.",
        Some("Improved wording"),
    )
    .await?;

    pm.tag("summarization", "stable", 1).await?;

    let latest = pm.latest("summarization").await?;
    println!("Latest prompt: {}", latest);

    let stable = pm
        .get_prompt("summarization", VersionSelector::Tag("stable"))
        .await?;
    println!("Stable prompt: {}", stable);

    pm.history("summarization").await?;

    pm.backup("backup.vault", Some("secure_pass")).await?;
    println!("✅ Vault backup done");

    let dev_prompt = pm
        .get_prompt("pc_operator_v2", VersionSelector::Tag("dev"))
        .await?;

    println!("Dev prompt: {}", dev_prompt);

    Ok(())
}
