//! Streaming, immutable filesystem CAS operations. A cache hit is always verified.

use crate::{CasError, Sha256Digest};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn path(&self, digest: &Sha256Digest) -> PathBuf {
        self.root.join(digest.sharded_path())
    }

    /// Missing is a cache miss; corruption is an error, never a cache miss.
    ///
    /// # Errors
    /// Returns I/O, unsafe/non-file path, or digest mismatch errors; a missing blob is `Ok(false)`.
    pub fn verified(&self, digest: &Sha256Digest) -> Result<bool, CasError> {
        self.check_parents(digest)?;
        let path = self.path(digest);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(CasError::InvalidSource(path))
            }
            Ok(_) => {}
        }
        let actual = digest_file(&path)?;
        if &actual != digest {
            return Err(CasError::DigestMismatch {
                artifact: path.display().to_string(),
                expected: digest.clone(),
                actual,
            });
        }
        Ok(true)
    }

    /// Publish verified bytes, returning whether a new blob was stored.
    ///
    /// # Errors
    /// Returns the same path, I/O, or digest errors as [`Self::put_reader`].
    pub fn put_bytes(&self, digest: &Sha256Digest, bytes: &[u8]) -> Result<bool, CasError> {
        self.put_reader(digest, &mut io::Cursor::new(bytes))
    }

    /// Stream a file into the store, returning whether a new blob was stored.
    ///
    /// # Errors
    /// Returns source-file I/O errors or the errors documented by [`Self::put_reader`].
    pub fn put_file(&self, digest: &Sha256Digest, path: &Path) -> Result<bool, CasError> {
        self.put_reader(digest, &mut File::open(path)?)
    }

    /// Stage and verify before publishing with no-clobber semantics.
    ///
    /// # Errors
    /// Returns reader/filesystem I/O, unsafe CAS path, or digest mismatch errors.
    /// A corrupt existing blob is an error, not a cache miss or an overwrite opportunity.
    pub fn put_reader(
        &self,
        digest: &Sha256Digest,
        reader: &mut impl Read,
    ) -> Result<bool, CasError> {
        if self.verified(digest)? {
            return Ok(false);
        }
        let path = self.path(digest);
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("CAS path has no parent"))?;
        self.prepare_parent(digest)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        io::copy(reader, &mut temporary)?;
        temporary.flush()?;
        let actual = digest_file(temporary.path())?;
        if &actual != digest {
            return Err(CasError::DigestMismatch {
                artifact: "incoming CAS blob".into(),
                expected: digest.clone(),
                actual,
            });
        }
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(&path) {
            Ok(_) => Ok(true),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                self.verified(digest)?;
                Ok(false)
            }
            Err(error) => Err(error.error.into()),
        }
    }

    fn check_parents(&self, digest: &Sha256Digest) -> Result<(), CasError> {
        let mut path = self.root.clone();
        for component in ["sha256", &digest.as_str()[..2], &digest.as_str()[2..4]] {
            path.push(component);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if !metadata.file_type().is_dir() => {
                    return Err(CasError::InvalidSource(path))
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error.into()),
                Ok(_) => {}
            }
        }
        Ok(())
    }

    fn prepare_parent(&self, digest: &Sha256Digest) -> Result<(), CasError> {
        fs::create_dir_all(&self.root)?;
        let mut path = self.root.clone();
        for component in ["sha256", &digest.as_str()[..2], &digest.as_str()[2..4]] {
            path.push(component);
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if !fs::symlink_metadata(&path)?.file_type().is_dir() {
                        return Err(CasError::InvalidSource(path));
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    /// Enumerate canonical blob paths only. Unexpected entries stop destructive GC.
    ///
    /// # Errors
    /// Returns I/O or invalid-source errors for noncanonical entries, symlinks, or non-files.
    pub fn digests(&self) -> Result<BTreeSet<String>, CasError> {
        let mut result = BTreeSet::new();
        let root = self.root.join("sha256");
        match fs::symlink_metadata(&root) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(result),
            Err(error) => return Err(error.into()),
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err(CasError::InvalidSource(root))
            }
            Ok(_) => {}
        }
        for first in fs::read_dir(root)? {
            let first = first?;
            check_prefix(&first)?;
            for second in fs::read_dir(first.path())? {
                let second = second?;
                check_prefix(&second)?;
                for file in fs::read_dir(second.path())? {
                    let file = file?;
                    let name = file
                        .file_name()
                        .to_str()
                        .ok_or_else(|| CasError::InvalidSource(file.path()))?
                        .to_owned();
                    let digest = Sha256Digest::parse(&name)
                        .map_err(|_| CasError::InvalidSource(file.path()))?;
                    if self.path(&digest) != file.path() || !file.file_type()?.is_file() {
                        return Err(CasError::InvalidSource(file.path()));
                    }
                    result.insert(name);
                }
            }
        }
        Ok(result)
    }
}

fn check_prefix(entry: &fs::DirEntry) -> Result<(), CasError> {
    let name = entry.file_name();
    if !entry.file_type()?.is_dir()
        || name.to_str().is_none_or(|name| {
            name.len() != 2
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
    {
        return Err(CasError::InvalidSource(entry.path()));
    }
    Ok(())
}

/// Hash a file with a bounded heap buffer.
///
/// # Errors
/// Returns an I/O error if opening or reading the file fails.
pub fn digest_file(path: &Path) -> Result<Sha256Digest, CasError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        hasher.update(&buffer[..length]);
    }
    Ok(Sha256Digest::from_hash_output(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_store_rejects_corruption_and_deduplicates() {
        let directory = tempfile::tempdir().unwrap();
        let store = FileStore::new(directory.path());
        let digest = Sha256Digest::from_bytes(b"verified");
        assert!(store.put_bytes(&digest, b"verified").unwrap());
        assert!(!store.put_bytes(&digest, b"verified").unwrap());
        assert_eq!(
            store.digests().unwrap(),
            BTreeSet::from([digest.as_str().to_owned()])
        );
        fs::write(store.path(&digest), b"corrupt").unwrap();
        assert!(store.verified(&digest).is_err());
        assert!(store.put_bytes(&digest, b"verified").is_err());
    }

    #[test]
    fn mismatched_incoming_bytes_are_never_published() {
        let directory = tempfile::tempdir().unwrap();
        let store = FileStore::new(directory.path());
        let digest = Sha256Digest::from_bytes(b"expected");
        assert!(store.put_bytes(&digest, b"wrong").is_err());
        assert!(!store.path(&digest).exists());
    }
}
