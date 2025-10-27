# Python bindings for the PromptPro Rust library
# This package provides Python access to the Rust-based prompt management system
import os
from typing import List, Optional, Union

try:
    # Try to import the compiled Rust extension
    from .promptpro import (
        PyPromptVault,
        PyVersionMeta,
        PySyncPromptManager,
        DefaultPromptManager,
    )

    # Create more Pythonic aliases
    PromptVault = PyPromptVault
    VersionMeta = PyVersionMeta
    PromptManager = PySyncPromptManager
except ImportError:
    # If the Rust extension isn't available, use the Python API wrapper
    from .api import PromptVault, VersionMeta, DefaultPromptManager, get_default_manager

__all__ = ["PromptVault", "VersionMeta", "PromptManager", "get_default_manager"]


def get_default_manager():
    """Get the default singleton prompt manager instance."""
    if "PySyncPromptManager" in globals():
        return PromptManager.get()
    else:
        # For the pure Python wrapper case
        from .api import get_default_manager as _get_default_manager

        return _get_default_manager()


class PromptManager:
    _instance = None  # for singleton

    def __init__(
        self, vault_bin_file: Optional[str] = None, password: Optional[str] = None
    ):
        if vault_bin_file and os.path.exists(vault_bin_file):
            if not os.path.isfile(vault_bin_file):
                raise ValueError(f"{vault_bin_file} vault bin must be a file!")
            # Load existing vault
            self._rust_vault = PromptVault.restore_or_default(vault_bin_file, password)
        else:
            # Create new vault
            self._rust_vault = PromptVault()

    # -------- Singleton accessor --------
    @staticmethod
    def get_singleton(
        vault_bin_file: Optional[str] = None, password: Optional[str] = None
    ):
        if PromptManager._instance is None:
            PromptManager._instance = PromptManager(vault_bin_file, password)
        return PromptManager._instance

    # -------- Prompt operations --------
    def add(self, key: str, content: str):
        """Add a new prompt."""
        self._rust_vault.add(key, content)

    def update(self, key: str, content: str, message: Optional[str] = None):
        """Update an existing prompt."""
        self._rust_vault.update(key, content, message)

    def tag(self, key: str, tag: str, version: int):
        """Tag a specific version of a prompt."""
        self._rust_vault.tag(key, tag, version)

    def get_prompt(self, key: str, selector: Union[str, int] = "latest") -> str:
        """Get a prompt by key and selector."""
        return self._rust_vault.get(key, selector)

    def latest(self, key: str) -> str:
        """Get the latest version of a prompt."""
        return self._rust_vault.latest(key)

    def history(self, key: str):
        """Get version history for a prompt."""
        rust_versions = self._rust_vault.history(key)
        return [VersionMeta(rv) for rv in rust_versions]

    def backup(self, path: str, password: Optional[str] = None):
        """Backup the vault to a file."""
        self._rust_vault.backup(path, password)
