#![forbid(unsafe_code)]
//! Content Addressable Storage (CAS) primitives and artifact digest verification.

mod sharding;
mod store;

pub use store::{digest_file, FileStore};

pub use sharding::{
    read_shard_manifest, shard_file, verify_shards, CasError, ShardManifest, ShardPart,
    SHARD_MANIFEST_SCHEMA_VERSION, SHARD_SIZE_BYTES,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// A 64-character lowercase hexadecimal SHA-256 digest string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Sha256Digest(String);

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl Sha256Digest {
    /// Computes a SHA-256 digest from raw bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self::from_hash_output(hasher.finalize())
    }

    pub(crate) fn from_hash_output(output: impl AsRef<[u8]>) -> Self {
        Self(hex::encode(output))
    }

    /// Validates and constructs a SHA-256 digest from a hexadecimal string.
    pub fn parse(s: &str) -> Result<Self, &'static str> {
        if s.len() != 64 {
            return Err("SHA-256 digest must be exactly 64 hexadecimal characters");
        }
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("SHA-256 digest must contain only hexadecimal characters");
        }
        Ok(Self(s.to_ascii_lowercase()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Computes CAS sharded path `sha256/<prefix2>/<prefix2>/<full_hash>`.
    #[must_use]
    pub fn sharded_path(&self) -> String {
        format!("sha256/{}/{}/{}", &self.0[0..2], &self.0[2..4], self.0)
    }
}

/// In-memory content-addressable storage map for airlock artifacts.
#[derive(Debug, Default, Clone)]
pub struct InMemoryCas {
    entries: HashMap<Sha256Digest, Vec<u8>>,
}

impl InMemoryCas {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Stores content only when its bytes match the supplied SHA-256 digest.
    pub fn insert(&mut self, digest: Sha256Digest, content: Vec<u8>) -> Result<(), CasError> {
        let actual = Sha256Digest::from_bytes(&content);
        if actual != digest {
            return Err(CasError::DigestMismatch {
                artifact: "in-memory CAS entry".into(),
                expected: digest,
                actual,
            });
        }
        self.entries.insert(digest, content);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, digest: &Sha256Digest) -> Option<&[u8]> {
        self.entries.get(digest).map(Vec::as_slice)
    }

    #[must_use]
    pub fn contains(&self, digest: &Sha256Digest) -> bool {
        self.entries.contains_key(digest)
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cas_digest_and_store() {
        let raw = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let from_empty = Sha256Digest::from_bytes(b"");
        assert_eq!(from_empty.as_str(), raw);
        let digest = Sha256Digest::parse(raw).expect("valid digest");
        assert_eq!(digest.as_str(), raw);
        assert_eq!(
            digest.sharded_path(),
            "sha256/e3/b0/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        let mut cas = InMemoryCas::new();
        assert!(!cas.contains(&digest));
        cas.insert(digest.clone(), Vec::new())
            .expect("matching content digest is accepted");
        assert!(cas.contains(&digest));
        assert_eq!(cas.get(&digest), Some(&[][..]));
        assert_eq!(cas.count(), 1);
    }

    #[test]
    fn in_memory_cas_rejects_content_with_a_mismatched_digest() {
        let expected = Sha256Digest::from_bytes(b"expected content");
        let mut cas = InMemoryCas::new();

        let result = cas.insert(expected, b"different content".to_vec());
        assert!(matches!(result, Err(CasError::DigestMismatch { .. })));
        assert_eq!(cas.count(), 0);
    }
}
