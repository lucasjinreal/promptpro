"""
Python API wrapper for PromptPro - A prompt versioning and management system

This module provides a more Pythonic interface to the underlying Rust implementation.
"""

from .promptpro import PyPromptVault, PyVersionMeta, PySyncPromptManager
from typing import Optional, Union, List
import os


class VersionMeta:
    """
    Represents metadata for a prompt version.

    Attributes:
        key: The key/name of the prompt
        version: The version number
        timestamp: The timestamp when the version was created
        parent: The parent version number (if any)
        message: Optional commit message for the version
        object_hash: The hash of the prompt content
        snapshot: Whether this version is stored as a snapshot (True) or diff (False)
        tags: List of tags associated with this version
    """

    def __init__(self, rust_meta):
        self._rust_meta = rust_meta
        # Expose all attributes from the Rust wrapper
        self.key = rust_meta.key
        self.version = rust_meta.version
        self.timestamp = rust_meta.timestamp
        self.parent = rust_meta.parent
        self.message = rust_meta.message
        self.object_hash = rust_meta.object_hash
        self.snapshot = rust_meta.snapshot
        self.tags = rust_meta.tags

    def __repr__(self):
        return f"VersionMeta(key={self.key}, version={self.version}, tags={self.tags})"

    def __str__(self):
        return f"v{self.version} - {self.timestamp} - tags: {self.tags}"


class PromptVault:
    """
    Main vault for storing and managing prompts with versioning.

    This class provides methods to add, update, retrieve, and manage prompts
    with full versioning and tagging support.
    """

    def __init__(self, path: Optional[str] = None):
        """
        Initialize a PromptVault.

        Args:
            path: Optional path to the vault directory. If None, uses default location.
        """
        self._rust_vault = PyPromptVault(path)

    def add(self, key: str, content: str):
        """
        Add a new prompt with the given key and content.

        Args:
            key: The key/name for the prompt
            content: The content of the prompt

        Raises:
            Exception: If a prompt with the given key already exists
        """
        self._rust_vault.add(key, content)

    def update(self, key: str, content: str, message: Optional[str] = None):
        """
        Update an existing prompt with new content.

        Args:
            key: The key of the prompt to update
            content: The new content for the prompt
            message: Optional message describing the update
        """
        self._rust_vault.update(key, content, message)

    def get(self, key: str, selector: Union[str, int] = "latest") -> str:
        """
        Get prompt content by key and selector.

        Args:
            key: The key of the prompt to retrieve
            selector: Version selector - can be:
                - "latest" for the latest version
                - An integer for a specific version number
                - A string for a tag name

        Returns:
            The content of the prompt
        """
        return self._rust_vault.get(key, selector)

    def get_latest(self, key: str) -> str:
        """
        Get the latest version of a prompt.

        Args:
            key: The key of the prompt to retrieve

        Returns:
            The content of the latest version
        """
        return self._rust_vault.get_latest(key)

    def history(self, key: str) -> List[VersionMeta]:
        """
        Get history of all versions for a key.

        Args:
            key: The key of the prompt

        Returns:
            A list of VersionMeta objects representing all versions
        """
        rust_versions = self._rust_vault.history(key)
        return [VersionMeta(rust_version) for rust_version in rust_versions]

    def tag(self, key: str, tag: str, version: int):
        """
        Tag a specific version of a prompt.

        Args:
            key: The key of the prompt
            tag: The tag name to apply
            version: The version number to tag
        """
        self._rust_vault.tag(key, tag, version)

    def promote(self, key: str, tag: str):
        """
        Promote a tag to point to the latest version.

        Args:
            key: The key of the prompt
            tag: The tag name to promote
        """
        self._rust_vault.promote(key, tag)

    def dump(self, output_path: str, password: Optional[str] = None):
        """
        Dump the vault to a binary file.

        Args:
            output_path: Path to the output file
            password: Optional password for encryption
        """
        self._rust_vault.dump(output_path, password)

    @staticmethod
    def restore(input_path: str, password: Optional[str] = None):
        """
        Restore a vault from a binary file.

        Args:
            input_path: Path to the input file
            password: Optional password for decryption

        Returns:
            A new PromptVault instance
        """
        rust_vault = PyPromptVault.restore(input_path, password)
        vault = PromptVault.__new__(PromptVault)
        vault._rust_vault = rust_vault
        return vault

    @staticmethod
    def restore_or_default(input_path: str, password: Optional[str] = None):
        """
        Restore a vault from a binary file, if file not found,
        loading from default.

        Args:
            input_path: Path to the input file
            password: Optional password for decryption

        Returns:
            A new PromptVault instance
        """
        rust_vault = PyPromptVault.restore_or_default(input_path, password)
        vault = PromptVault()
        vault._rust_vault = rust_vault
        return vault

    def get_latest_version_number(self, key: str) -> Optional[int]:
        """
        Get the latest version number for a key.

        Args:
            key: The key of the prompt

        Returns:
            The latest version number, or None if no versions exist
        """
        return self._rust_vault.get_latest_version_number(key)

    def delete(self, key: str):
        """
        Delete a prompt key and all its versions.

        Args:
            key: The key of the prompt to delete
        """
        self._rust_vault.delete(key)


class DefaultPromptManager:
    """
    A manager for handling prompt operations with convenient methods.

    This class provides a high-level interface for prompt management.
    """

    def __init__(self, path: Optional[str] = None):
        """
        Initialize a PromptManager.

        Args:
            path: Optional path to use for the underlying vault
        """
        self._rust_manager = PySyncPromptManager(path)

    @staticmethod
    def get_singleton():  # Returns the singleton instance
        """
        Get the singleton prompt manager instance.

        Returns:
            The global singleton PromptManager instance
        """
        rust_manager = PySyncPromptManager.get()
        manager = DefaultPromptManager.__new__(DefaultPromptManager)
        manager._rust_manager = rust_manager
        return manager

    def add(self, key: str, content: str):
        """
        Add a new prompt.

        Args:
            key: The key/name for the prompt
            content: The content of the prompt
        """
        self._rust_manager.add(key, content)

    def update(self, key: str, content: str, message: Optional[str] = None):
        """
        Update an existing prompt.

        Args:
            key: The key of the prompt to update
            content: The new content for the prompt
            message: Optional message describing the update
        """
        self._rust_manager.update(key, content, message)

    def tag(self, key: str, tag: str, version: int):
        """
        Tag a specific version of a prompt.

        Args:
            key: The key of the prompt
            tag: The tag name to apply
            version: The version number to tag
        """
        self._rust_manager.tag(key, tag, version)

    def get_prompt(self, key: str, selector: Union[str, int] = "latest") -> str:
        """
        Get a prompt by key and selector.

        Args:
            key: The key of the prompt
            selector: Version selector - can be "latest", a version number, or a tag

        Returns:
            The content of the prompt
        """
        return self._rust_manager.get_prompt(key, selector)

    def latest(self, key: str) -> str:
        """
        Get the latest version of a prompt.

        Args:
            key: The key of the prompt

        Returns:
            The content of the latest version
        """
        return self._rust_manager.latest(key)

    def history(self, key: str) -> List[VersionMeta]:
        """
        Get history of a prompt.

        Args:
            key: The key of the prompt

        Returns:
            A list of VersionMeta objects representing all versions
        """
        rust_versions = self._rust_manager.history(key)
        return [VersionMeta(rust_version) for rust_version in rust_versions]

    def backup(self, path: str, password: Optional[str] = None):
        """
        Backup the vault to a file.

        Args:
            path: Path to the backup file
            password: Optional password for encryption
        """
        self._rust_manager.backup(path, password)

    def delete(self, key: str):
        """
        Delete a prompt key and all its versions.

        Args:
            key: The key of the prompt to delete
        """
        self._rust_manager.delete(key)


# For backward compatibility
def get_default_manager() -> DefaultPromptManager:
    """
    Get the default singleton prompt manager instance.

    Returns:
        The global PromptManager instance
    """
    return DefaultPromptManager.get_singleton()
