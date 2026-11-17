//! Streaming file sharding and integrity verification for large artifacts.

use crate::Sha256Digest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

/// Maximum size of a single stored blob shard: 95 MiB.
pub const SHARD_SIZE_BYTES: u64 = 95 * 1024 * 1024;

/// Schema version written in each shard manifest.
pub const SHARD_MANIFEST_SCHEMA_VERSION: u32 = 1;

const IO_BUFFER_SIZE: usize = 64 * 1024;

/// A manifest describing a large artifact and its sequential shard files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShardManifest {
    /// Manifest format version.
    pub schema_version: u32,
    /// Basename of the original artifact.
    pub original_filename: String,
    /// Size of the original artifact in bytes.
    pub original_size_bytes: u64,
    /// SHA-256 digest of the original artifact.
    pub original_sha256: Sha256Digest,
    /// Maximum size of each part in bytes.
    pub chunk_size_bytes: u64,
    /// Parts in concatenation order.
    pub chunks: Vec<ShardPart>,
}

impl ShardManifest {
    /// Returns the manifest filename for this artifact.
    #[must_use]
    pub fn manifest_filename(&self) -> String {
        format!("{}.shards.json", self.original_filename)
    }

    /// Checks that the manifest describes a canonical, complete shard sequence.
    pub fn validate(&self) -> Result<(), CasError> {
        validate_filename(&self.original_filename)?;
        if self.schema_version != SHARD_MANIFEST_SCHEMA_VERSION {
            return Err(CasError::InvalidManifest(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        if self.original_size_bytes <= SHARD_SIZE_BYTES {
            return Err(CasError::InvalidManifest(
                "manifest is only valid for artifacts larger than the shard ceiling".into(),
            ));
        }
        if self.chunk_size_bytes != SHARD_SIZE_BYTES {
            return Err(CasError::InvalidManifest(format!(
                "chunk size must be {SHARD_SIZE_BYTES} bytes"
            )));
        }

        let expected_count = chunk_count(self.original_size_bytes, self.chunk_size_bytes);
        let actual_count = u64::try_from(self.chunks.len()).map_err(|_| {
            CasError::InvalidManifest("part count does not fit in a 64-bit index".into())
        })?;
        if actual_count != expected_count {
            return Err(CasError::InvalidManifest(format!(
                "expected {expected_count} parts but found {actual_count}"
            )));
        }

        let mut verified_size = 0_u64;
        for (position, part) in self.chunks.iter().enumerate() {
            let expected_index = u64::try_from(position).map_err(|_| {
                CasError::InvalidManifest("part index does not fit in a 64-bit index".into())
            })?;
            if part.index != expected_index {
                return Err(CasError::InvalidManifest(format!(
                    "part at position {position} has index {}",
                    part.index
                )));
            }
            let expected_filename =
                part_filename(&self.original_filename, part.index, expected_count);
            if part.part_filename != expected_filename {
                return Err(CasError::InvalidManifest(format!(
                    "part {} must be named {expected_filename}",
                    part.index
                )));
            }
            let remaining = self.original_size_bytes - verified_size;
            let expected_size = remaining.min(self.chunk_size_bytes);
            if part.size_bytes != expected_size {
                return Err(CasError::InvalidManifest(format!(
                    "part {} must contain {expected_size} bytes, not {}",
                    part.index, part.size_bytes
                )));
            }
            verified_size = verified_size.checked_add(part.size_bytes).ok_or_else(|| {
                CasError::InvalidManifest("sum of part sizes overflows u64".into())
            })?;
        }
        if verified_size != self.original_size_bytes {
            return Err(CasError::InvalidManifest(format!(
                "parts total {verified_size} bytes but original size is {}",
                self.original_size_bytes
            )));
        }
        Ok(())
    }
}

/// Metadata for one sequential part in a [`ShardManifest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShardPart {
    /// Zero-based position in the concatenation order.
    pub index: u64,
    /// Safe basename of the corresponding part file.
    pub part_filename: String,
    /// Part size in bytes.
    pub size_bytes: u64,
    /// SHA-256 digest of this part.
    pub sha256: Sha256Digest,
}

/// Errors from CAS file operations, manifest parsing, and integrity validation.
#[derive(Debug)]
pub enum CasError {
    /// An operating-system file or stream error.
    Io(io::Error),
    /// Invalid or malformed JSON manifest data.
    Json(serde_json::Error),
    /// The artifact filename cannot be represented as a safe portable basename.
    InvalidFilename(String),
    /// The source is not a regular file.
    InvalidSource(PathBuf),
    /// A manifest violates its schema or sequence invariants.
    InvalidManifest(String),
    /// The source changed while it was being sharded.
    SourceChanged {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    /// A stored part or reconstructed artifact has the wrong size.
    SizeMismatch {
        artifact: String,
        expected: u64,
        actual: u64,
    },
    /// A stored part or reconstructed artifact has the wrong digest.
    DigestMismatch {
        artifact: String,
        expected: Sha256Digest,
        actual: Sha256Digest,
    },
}

impl fmt::Display for CasError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "CAS I/O error: {error}"),
            Self::Json(error) => write!(formatter, "invalid shard manifest JSON: {error}"),
            Self::InvalidFilename(filename) => {
                write!(
                    formatter,
                    "artifact filename is not a safe basename: {filename:?}"
                )
            }
            Self::InvalidSource(path) => {
                write!(formatter, "source {} is not a regular file", path.display())
            }
            Self::InvalidManifest(reason) => write!(formatter, "invalid shard manifest: {reason}"),
            Self::SourceChanged {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "source {} changed while sharding (expected {expected} bytes, read {actual})",
                path.display()
            ),
            Self::SizeMismatch {
                artifact,
                expected,
                actual,
            } => write!(
                formatter,
                "size mismatch for {artifact}: expected {expected} bytes, found {actual}"
            ),
            Self::DigestMismatch {
                artifact,
                expected,
                actual,
            } => write!(
                formatter,
                "SHA-256 mismatch for {artifact}: expected {}, found {}",
                expected.as_str(),
                actual.as_str()
            ),
        }
    }
}

impl Error for CasError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CasError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for CasError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Splits `source_path` into `.partNNN` files when it exceeds 95 MiB.
///
/// Parts and a `<filename>.shards.json` manifest are written to `shards_directory`.
/// The source is streamed using a fixed-size buffer; artifacts at or below the limit
/// return `Ok(None)` and do not create the destination directory.
pub fn shard_file(
    source_path: impl AsRef<Path>,
    shards_directory: impl AsRef<Path>,
) -> Result<Option<ShardManifest>, CasError> {
    let source_path = source_path.as_ref();
    let mut source = File::open(source_path)?;
    let metadata = source.metadata()?;
    if !metadata.is_file() {
        return Err(CasError::InvalidSource(source_path.to_path_buf()));
    }
    let original_size_bytes = metadata.len();
    if original_size_bytes <= SHARD_SIZE_BYTES {
        return Ok(None);
    }

    let original_filename = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CasError::InvalidFilename(source_path.display().to_string()))?
        .to_owned();
    validate_filename(&original_filename)?;

    let shards_directory = shards_directory.as_ref();
    fs::create_dir_all(shards_directory)?;
    let mut created_files = Vec::new();
    let result = write_shard_set(
        &mut source,
        source_path,
        shards_directory,
        original_filename,
        original_size_bytes,
        &mut created_files,
    );

    if result.is_err() {
        for path in created_files.into_iter().rev() {
            let _ = fs::remove_file(path);
        }
    }
    result.map(Some)
}

struct ShardWriteContext<'a> {
    source: &'a mut File,
    source_path: &'a Path,
    original_size_bytes: u64,
    total_read: u64,
    original_hasher: Sha256,
    created_files: &'a mut Vec<PathBuf>,
}

fn write_shard_set(
    source: &mut File,
    source_path: &Path,
    shards_directory: &Path,
    original_filename: String,
    original_size_bytes: u64,
    created_files: &mut Vec<PathBuf>,
) -> Result<ShardManifest, CasError> {
    let mut context = ShardWriteContext {
        source,
        source_path,
        original_size_bytes,
        total_read: 0,
        original_hasher: Sha256::new(),
        created_files,
    };
    let count = chunk_count(original_size_bytes, SHARD_SIZE_BYTES);
    let mut chunks = Vec::new();

    for index in 0..count {
        let chunk_size = (original_size_bytes - context.total_read).min(SHARD_SIZE_BYTES);
        let filename = part_filename(&original_filename, index, count);
        chunks.push(write_shard_part(
            &mut context,
            shards_directory,
            index,
            filename,
            chunk_size,
        )?);
    }

    verify_source_unchanged(context.source, context.source_path, original_size_bytes)?;
    let manifest = ShardManifest {
        schema_version: SHARD_MANIFEST_SCHEMA_VERSION,
        original_filename,
        original_size_bytes,
        original_sha256: Sha256Digest::from_hash_output(context.original_hasher.finalize()),
        chunk_size_bytes: SHARD_SIZE_BYTES,
        chunks,
    };
    manifest.validate()?;
    write_manifest(shards_directory, &manifest, context.created_files)?;
    Ok(manifest)
}

fn write_shard_part(
    context: &mut ShardWriteContext<'_>,
    shards_directory: &Path,
    index: u64,
    filename: String,
    chunk_size: u64,
) -> Result<ShardPart, CasError> {
    let path = shards_directory.join(&filename);
    let mut part_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    context.created_files.push(path);

    let mut part_hasher = Sha256::new();
    let mut part_read = 0_u64;
    let mut buffer = [0_u8; IO_BUFFER_SIZE];
    while part_read < chunk_size {
        let read_limit = (chunk_size - part_read).min(buffer.len() as u64) as usize;
        let read = context.source.read(&mut buffer[..read_limit])?;
        if read == 0 {
            return Err(CasError::SourceChanged {
                path: context.source_path.to_path_buf(),
                expected: context.original_size_bytes,
                actual: context.total_read,
            });
        }
        part_file.write_all(&buffer[..read])?;
        part_hasher.update(&buffer[..read]);
        context.original_hasher.update(&buffer[..read]);
        part_read += read as u64;
        context.total_read += read as u64;
    }
    part_file.flush()?;
    part_file.sync_all()?;

    Ok(ShardPart {
        index,
        part_filename: filename,
        size_bytes: part_read,
        sha256: Sha256Digest::from_hash_output(part_hasher.finalize()),
    })
}

fn verify_source_unchanged(
    source: &mut File,
    source_path: &Path,
    expected_size: u64,
) -> Result<(), CasError> {
    let mut extra_byte = [0_u8; 1];
    if source.read(&mut extra_byte)? != 0 {
        return Err(CasError::SourceChanged {
            path: source_path.to_path_buf(),
            expected: expected_size,
            actual: expected_size.saturating_add(1),
        });
    }
    let actual_size = source.metadata()?.len();
    if actual_size != expected_size {
        return Err(CasError::SourceChanged {
            path: source_path.to_path_buf(),
            expected: expected_size,
            actual: actual_size,
        });
    }
    Ok(())
}

fn write_manifest(
    shards_directory: &Path,
    manifest: &ShardManifest,
    created_files: &mut Vec<PathBuf>,
) -> Result<(), CasError> {
    let path = shards_directory.join(manifest.manifest_filename());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    created_files.push(path);
    serde_json::to_writer_pretty(&mut file, manifest)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

/// Reads and validates a shard manifest from JSON.
pub fn read_shard_manifest(path: impl AsRef<Path>) -> Result<ShardManifest, CasError> {
    let file = File::open(path)?;
    let manifest: ShardManifest = serde_json::from_reader(BufReader::new(file))?;
    manifest.validate()?;
    Ok(manifest)
}

/// Verifies every part and the digest of their ordered concatenation.
///
/// Consumers can reassemble the payload with POSIX `cat` over the part filenames
/// in the manifest; this function checks the same ordered byte stream without
/// buffering the complete artifact in memory.
pub fn verify_shards(
    shards_directory: impl AsRef<Path>,
    manifest: &ShardManifest,
) -> Result<(), CasError> {
    manifest.validate()?;
    let shards_directory = shards_directory.as_ref();
    let mut original_hasher = Sha256::new();
    let mut total_size = 0_u64;

    for part in &manifest.chunks {
        let path = shards_directory.join(&part.part_filename);
        let mut file = File::open(&path)?;
        let actual_file_size = file.metadata()?.len();
        if actual_file_size != part.size_bytes {
            return Err(CasError::SizeMismatch {
                artifact: part.part_filename.clone(),
                expected: part.size_bytes,
                actual: actual_file_size,
            });
        }
        let (actual_size, actual_digest) =
            hash_part(&mut file, part.size_bytes, &mut original_hasher)?;
        if actual_size != part.size_bytes {
            return Err(CasError::SizeMismatch {
                artifact: part.part_filename.clone(),
                expected: part.size_bytes,
                actual: actual_size,
            });
        }
        if actual_digest != part.sha256 {
            return Err(CasError::DigestMismatch {
                artifact: part.part_filename.clone(),
                expected: part.sha256.clone(),
                actual: actual_digest,
            });
        }
        total_size = total_size.checked_add(actual_size).ok_or_else(|| {
            CasError::InvalidManifest("sum of verified part sizes overflows u64".into())
        })?;
    }

    if total_size != manifest.original_size_bytes {
        return Err(CasError::SizeMismatch {
            artifact: manifest.original_filename.clone(),
            expected: manifest.original_size_bytes,
            actual: total_size,
        });
    }
    let actual_original_digest = Sha256Digest::from_hash_output(original_hasher.finalize());
    if actual_original_digest != manifest.original_sha256 {
        return Err(CasError::DigestMismatch {
            artifact: manifest.original_filename.clone(),
            expected: manifest.original_sha256.clone(),
            actual: actual_original_digest,
        });
    }
    Ok(())
}

fn hash_part<R: Read>(
    reader: &mut R,
    expected_size: u64,
    original_hasher: &mut Sha256,
) -> Result<(u64, Sha256Digest), CasError> {
    let mut part_hasher = Sha256::new();
    let mut size = 0_u64;
    let read_limit = expected_size.saturating_add(1);
    let mut buffer = [0_u8; IO_BUFFER_SIZE];
    while size < read_limit {
        let length = (read_limit - size).min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..length])?;
        if read == 0 {
            break;
        }
        part_hasher.update(&buffer[..read]);
        original_hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((size, Sha256Digest::from_hash_output(part_hasher.finalize())))
}

fn chunk_count(total_size: u64, chunk_size: u64) -> u64 {
    total_size.div_ceil(chunk_size)
}

fn part_filename(original_filename: &str, index: u64, part_count: u64) -> String {
    let width = part_count.saturating_sub(1).to_string().len().max(3);
    format!("{original_filename}.part{:0width$}", index, width = width)
}

fn validate_filename(filename: &str) -> Result<(), CasError> {
    if filename.is_empty()
        || filename == "."
        || filename == ".."
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains('\0')
    {
        return Err(CasError::InvalidFilename(filename.to_owned()));
    }
    Ok(())
}
