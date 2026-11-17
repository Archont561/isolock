#![forbid(unsafe_code)]
//! Content Addressable Storage (CAS) primitives and artifact digest verification.

use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// A 64-character lowercase hexadecimal SHA-256 digest string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    /// Computes a SHA-256 digest from raw bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let result = hasher.finalize();
        Self(hex::encode(result))
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

    pub fn insert(&mut self, digest: Sha256Digest, content: Vec<u8>) {
        self.entries.insert(digest, content);
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
        cas.insert(digest.clone(), b"empty_payload".to_vec());
        assert!(cas.contains(&digest));
        assert_eq!(cas.get(&digest), Some(&b"empty_payload"[..]));
        assert_eq!(cas.count(), 1);
    }
}
