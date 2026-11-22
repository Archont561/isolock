//! Git plumbing with a private index; the code checkout and its refs are never staged.

use super::manifest::MAX_MANIFEST_BYTES;
use isolock_cas::{Sha256Digest, SHARD_SIZE_BYTES};
use isolock_plan::profile::{identifier, public_source};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MARKER_PATH: &str = "isolock-storage.json";
const MARKER: &[u8] = b"{\"schema-version\":1,\"format\":\"reviewed-artifacts\"}\n";
const MAX_TREE_BYTES: u64 = 1500 * 1024 * 1024;

pub struct Publisher {
    root: PathBuf,
    staging: tempfile::TempDir,
    remote: String,
    reference: String,
    parent: Option<String>,
    entries: BTreeMap<String, String>,
}

impl Publisher {
    pub fn open(root: &Path, remote: &str, reference: &str) -> Result<Self, String> {
        let generation = reference.strip_prefix("isolock/v").unwrap_or_default();
        if generation.starts_with('0')
            || generation.is_empty()
            || !generation.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("publication requires a storage reference isolock/vN; code branches and tags are forbidden".into());
        }
        let mut publisher = Self {
            root: root.into(),
            staging: tempfile::tempdir().map_err(|_| "cannot create a private Git index")?,
            remote: String::new(),
            reference: format!("refs/heads/{reference}"),
            parent: None,
            entries: BTreeMap::new(),
        };
        publisher.run(&["rev-parse", "--git-dir"], "repository discovery")?;
        publisher.remote = publisher.remote(remote)?;
        publisher.fetch_parent()?;
        if let Some(parent) = publisher.parent.as_deref() {
            let tree = publisher.run(&["ls-tree", "-rlz", parent], "storage tree inspection")?;
            publisher.entries = parse_tree(&tree)?;
            if publisher.read(MARKER_PATH)?.as_deref() != Some(MARKER) {
                return Err(
                    "storage reference is not an initialized reviewed-artifact snapshot".into(),
                );
            }
            publisher.run(&["read-tree", parent], "private index initialization")?;
        } else {
            publisher.run(&["read-tree", "--empty"], "private index initialization")?;
        }
        publisher.put_bytes(MARKER_PATH, MARKER, false)?;
        Ok(publisher)
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(&self.root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env("GIT_INDEX_FILE", self.staging.path().join("index"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(arguments);
        command
    }

    fn output(&self, arguments: &[&str]) -> Result<Output, String> {
        self.command(arguments)
            .output()
            .map_err(|_| "cannot start Git publication".into())
    }

    pub fn run(&self, arguments: &[&str], operation: &str) -> Result<Vec<u8>, String> {
        let output = self.output(arguments)?;
        if !output.status.success() {
            return Err(format!("Git {operation} failed; check repository access, author configuration, and storage history"));
        }
        Ok(output.stdout)
    }

    fn remote(&self, remote: &str) -> Result<String, String> {
        if let Some(source) = self.local_or_https(remote)? {
            return Ok(source);
        }
        identifier(remote).map_err(|_| "invalid publication remote")?;
        if remote.starts_with('-') {
            return Err("invalid publication remote".into());
        }
        let bytes = self.run(&["remote", "get-url", "--push", remote], "remote lookup")?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| "remote URL is not UTF-8")?
            .trim();
        self.local_or_https(source)?.ok_or_else(|| {
            "publication remote must be a local directory or public HTTPS URL".into()
        })
    }

    fn local_or_https(&self, source: &str) -> Result<Option<String>, String> {
        public_source(source)?;
        if source.starts_with('-') || source.contains(['\\', '\t']) {
            return Err("invalid publication remote".into());
        }
        if source.starts_with("https://") {
            if source
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
            {
                return Err("invalid publication remote".into());
            }
            return Ok(Some(source.into()));
        }
        let path = self.root.join(source);
        if path.is_dir() {
            let path =
                fs::canonicalize(path).map_err(|_| "cannot resolve publication directory")?;
            return Ok(Some(
                path.to_str()
                    .ok_or("publication directory is not UTF-8")?
                    .into(),
            ));
        }
        Ok(None)
    }

    fn fetch_parent(&mut self) -> Result<(), String> {
        let output = self.output(&[
            "ls-remote",
            "--exit-code",
            "--heads",
            "--",
            &self.remote,
            &self.reference,
        ])?;
        if output.status.code() == Some(2) {
            return Ok(());
        }
        if !output.status.success() {
            return Err(
                "cannot inspect storage remote; a failed lookup is not an empty branch".into(),
            );
        }
        self.run(
            &[
                "fetch",
                "--quiet",
                "--no-tags",
                "--depth=1",
                "--",
                &self.remote,
                &self.reference,
            ],
            "storage fetch",
        )?;
        self.parent = Some(object_id(&self.run(
            &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
            "storage commit lookup",
        )?)?);
        Ok(())
    }

    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
        if !self.entries.contains_key(path) {
            return Ok(None);
        }
        let parent = self
            .parent
            .as_ref()
            .ok_or("storage parent is unavailable")?;
        let object = format!("{parent}:{path}");
        let size = self.run(&["cat-file", "-s", &object], "storage object inspection")?;
        if std::str::from_utf8(&size)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .is_none_or(|size| size > MAX_MANIFEST_BYTES)
        {
            return Err("storage metadata exceeds 4 MiB".into());
        }
        self.run(&["show", &object], "storage metadata retrieval")
            .map(Some)
    }

    pub fn put_bytes(&mut self, path: &str, bytes: &[u8], mutable: bool) -> Result<(), String> {
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err("storage metadata exceeds 4 MiB".into());
        }
        let temporary = self.staging.path().join("metadata");
        fs::write(&temporary, bytes).map_err(|_| "cannot stage storage metadata")?;
        self.put_file(path, &temporary, mutable)
    }

    pub fn put_file(&mut self, path: &str, source: &Path, mutable: bool) -> Result<(), String> {
        if !allowed_path(path) {
            return Err("invalid storage snapshot path".into());
        }
        let metadata =
            fs::symlink_metadata(source).map_err(|_| "cannot inspect publication input")?;
        if !metadata.file_type().is_file() || metadata.len() > SHARD_SIZE_BYTES {
            return Err("Git publication requires regular originals no larger than 95 MiB; sharded publication is not implemented".into());
        }
        let source = source
            .to_str()
            .ok_or("publication input path is not UTF-8")?;
        let object = object_id(&self.run(&["hash-object", "-w", "--", source], "blob staging")?)?;
        if self
            .entries
            .get(path)
            .is_some_and(|existing| existing != &object)
            && !mutable
        {
            return Err(
                "refusing to change an immutable published artifact or review record".into(),
            );
        }
        self.run(
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "100644",
                &object,
                path,
            ],
            "private index staging",
        )?;
        self.entries.insert(path.into(), object);
        Ok(())
    }

    pub fn pinned_source(&self, source: &str, commit: &str) -> Result<(), String> {
        let commit = object_id(commit.as_bytes())?;
        if self.local_or_https(source)?.as_deref() != Some(&self.remote) {
            return Err("selected profile is locked to another storage source".into());
        }
        self.run(
            &[
                "fetch",
                "--quiet",
                "--no-tags",
                "--depth=1",
                "--",
                &self.remote,
                &commit,
            ],
            "locked payload commit fetch",
        )?;
        Ok(())
    }

    pub fn verify_pinned_blob(
        &self,
        commit: &str,
        relative: &str,
        source: &Path,
    ) -> Result<(), String> {
        let source = source
            .to_str()
            .ok_or("publication input path is not UTF-8")?;
        let expected =
            object_id(&self.run(&["hash-object", "--", source], "payload verification")?)?;
        let actual = object_id(&self.run(
            &["rev-parse", "--verify", &format!("{commit}:{relative}")],
            "locked payload lookup (publish reviewed originals before locking a profile)",
        )?)?;
        if actual != expected {
            return Err(
                "locked storage commit does not contain the selected verified payload bytes".into(),
            );
        }
        Ok(())
    }

    pub fn finish(self, message: &str) -> Result<String, String> {
        let tree = object_id(&self.run(&["write-tree"], "storage tree creation")?)?;
        // Keep a bounded live snapshot. Automatic epoch migration is a separate feature.
        parse_tree(&self.run(&["ls-tree", "-rlz", &tree], "storage budget inspection")?)?;
        if let Some(parent) = self.parent.as_deref() {
            let old_tree = object_id(&self.run(
                &["rev-parse", &format!("{parent}^{{tree}}")],
                "parent tree lookup",
            )?)?;
            if old_tree == tree {
                return Ok(format!("Storage is up to date: {parent}"));
            }
        }
        let mut arguments = vec!["commit-tree", &tree];
        if let Some(parent) = self.parent.as_deref() {
            arguments.extend(["-p", parent]);
        }
        arguments.extend(["-m", message]);
        let commit = object_id(&self.run(&arguments, "audit commit creation")?)?;
        self.run(
            &[
                "push",
                "--porcelain",
                "--",
                &self.remote,
                &format!("{commit}:{}", self.reference),
            ],
            "non-force publication (remote may have advanced; retry from its new tip)",
        )?;
        Ok(format!("Published {commit} to {}", self.reference))
    }
}

fn object_id(bytes: &[u8]) -> Result<String, String> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| "Git object ID is not UTF-8")?
        .trim();
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid Git object ID".into());
    }
    Ok(value.into())
}

fn parse_tree(bytes: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let mut entries = BTreeMap::new();
    let mut total = 0_u64;
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let entry = std::str::from_utf8(entry).map_err(|_| "storage tree is not UTF-8")?;
        let (metadata, path) = entry.split_once('\t').ok_or("invalid storage tree entry")?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 4 || fields[0] != "100644" || fields[1] != "blob" || !allowed_path(path)
        {
            return Err("storage reference contains non-snapshot paths or unsafe modes".into());
        }
        let size = fields[3]
            .parse::<u64>()
            .map_err(|_| "invalid storage blob size")?;
        total = total.checked_add(size).ok_or("storage size overflow")?;
        if size > SHARD_SIZE_BYTES || total > MAX_TREE_BYTES {
            return Err("storage snapshot exceeds publication limits; use directory transport or an explicitly reviewed new epoch".into());
        }
        entries.insert(path.into(), object_id(fields[2].as_bytes())?);
    }
    Ok(entries)
}

fn allowed_path(path: &str) -> bool {
    if matches!(path, MARKER_PATH | "plan.json") {
        return true;
    }
    if let Some(relative) = path.strip_prefix("cas/") {
        return relative
            .rsplit('/')
            .next()
            .and_then(|name| Sha256Digest::parse(name).ok())
            .is_some_and(|digest| digest.sharded_path() == relative);
    }
    for prefix in ["ingestion/objects/", "ingestion/records/", "plans/objects/"] {
        if let Some(name) = path
            .strip_prefix(prefix)
            .and_then(|name| name.strip_suffix(".json"))
        {
            return Sha256Digest::parse(name).is_ok_and(|digest| digest.as_str() == name);
        }
    }
    path.strip_prefix("plans/")
        .and_then(|name| name.strip_suffix(".lock"))
        .is_some_and(|name| identifier(name).is_ok())
}
