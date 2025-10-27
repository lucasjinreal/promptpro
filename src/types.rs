use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Metadata for a prompt version
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VersionMeta {
    pub key: String,
    pub version: u64,
    pub timestamp: DateTime<Utc>,
    pub parent: Option<u64>,
    pub message: Option<String>,
    pub object_hash: String,
    pub snapshot: bool,
    pub tags: Vec<String>,
}

impl VersionMeta {
    pub fn new(key: String, version: u64, content: &str, parent: Option<u64>, message: Option<String>) -> Self {
        let object_hash = calculate_hash(content);
        let timestamp = Utc::now();
        let tags = Vec::new();
        
        VersionMeta {
            key,
            version,
            timestamp,
            parent,
            message,
            object_hash,
            snapshot: true, // Initially all versions are snapshots
            tags,
        }
    }
}

/// Calculate a hash for the content to detect changes
fn calculate_hash(content: &str) -> String {
    let hash = blake3::hash(content.as_bytes());
    format!("{}", hash)
}

/// Selector for getting specific versions of prompts
#[derive(Debug, Clone)]
pub enum VersionSelector<'a> {
    Latest,
    Version(u64),
    Tag(&'a str),
    Time(DateTime<Utc>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_meta_creation() {
        let meta = VersionMeta::new(
            "test_key".to_string(),
            1,
            "test content",
            None,
            Some("initial version".to_string()),
        );

        assert_eq!(meta.key, "test_key");
        assert_eq!(meta.version, 1);
        assert_eq!(meta.message, Some("initial version".to_string()));
        assert!(!meta.object_hash.is_empty());
        assert_eq!(meta.tags.len(), 0);
    }

    #[test]
    fn test_hash_calculation() {
        let content1 = "hello world";
        let content2 = "hello world";
        let content3 = "different content";

        assert_eq!(calculate_hash(content1), calculate_hash(content2));
        assert_ne!(calculate_hash(content1), calculate_hash(content3));
    }
}