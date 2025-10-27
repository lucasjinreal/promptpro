#!/usr/bin/env python3
"""
Example Python script demonstrating the usage of PromptPro Python bindings.
"""

import tempfile
import os
from promptpro import PromptManager, PromptVault


def test_low_level():
    """
    showing lowlevel API to use promptpro
    """

    vault = PromptVault()

    vault.add("summarization", "Summarize the following text...")
    vault.update(
        "summarization",
        "Provide a concise summary of the text, keeping context.",
        "Improved summarization prompt",
    )

    vault.tag("summarization", "stable", 1)
    latest = vault.get("summarization")
    stable = vault.get("summarization", "stable")

    # Show history
    history = vault.history("summarization")
    for version in history:
        print(f"  v{version.version}: {version.timestamp} - tags: {version.tags}")

    vault.update(
        "summarization",
        "Provide a concise summary of the text, keeping context and highlighting key facts.",
        "Added fact highlighting",
    )

    latest = vault.get("summarization")
    latest_version = vault.get_latest_version_number("summarization")
    if latest_version:
        vault.tag("summarization", "dev", latest_version)
        print(f"✅ Tagged latest version ({latest_version}) as dev")

    dev_version = vault.get("summarization", "dev")
    with tempfile.NamedTemporaryFile(suffix=".vault", delete=False) as tmp_file:
        backup_path = tmp_file.name

    try:
        vault.dump(backup_path, "backup_password")

        restored_vault = PromptVault.restore(backup_path, "backup_password")
        restored_content = restored_vault.get("summarization")

    except Exception as e:
        print(f"⚠️  Backup/restore failed: {e}")
    finally:
        if os.path.exists(backup_path):
            os.unlink(backup_path)  # Clean up temp file
            print("🧹 Cleaned up backup file")


def test_default():
    """
    mostly used high level API, simple to use.
    other features you can use in ppro tui
    """
    # pm = PromptManager.get_singleton("promptpro.vault", "")
    pm = PromptManager("promptpro.vault", "")
    a = pm.get_prompt("pc_operator_v2", "dev")
    print(a)


if __name__ == "__main__":
    test_default()
