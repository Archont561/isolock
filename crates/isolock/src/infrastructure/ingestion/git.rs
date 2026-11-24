//! Git plumbing in a disposable repository and private index, never the code checkout.

use super::manifest::MAX_MANIFEST_BYTES;
use crate::application::ports::{ProcessOutput, ProcessRequest, ProcessRunner};
use isolock_cas::{Sha256Digest, SHARD_SIZE_BYTES};
use isolock_plan::profile::{identifier, public_source};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) const MARKER_PATH: &str = "isolock-storage.json";
pub(super) const MARKER: &[u8] = b"{\"schema-version\":1,\"format\":\"reviewed-artifacts\"}\n";
pub(super) const MAX_TREE_BYTES: u64 = 1500 * 1024 * 1024;
/// Variables that would redirect Git away from the repository this publisher owns.
const CLEAN_GIT: [&str; 7] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_SHALLOW_FILE",
];

const MIGRATION: &str = "reviewed epoch migration requires a marked, verified-raw reviewed-artifact snapshot; re-ingest original files using a reviewed manifest into an empty isolock/vN (use directory transport for sharded originals)";

#[derive(Clone)]
pub(super) struct Blob {
    pub object: String,
    pub size: u64,
}

pub struct Publisher<'a> {
    processes: &'a dyn ProcessRunner,
    root: PathBuf,
    staging: tempfile::TempDir,
    hooks: PathBuf,
    remote: String,
    pub(super) reference: String,
    pub(super) parent: Option<String>,
    pub(super) entries: BTreeMap<String, Blob>,
}

impl<'a> Publisher<'a> {
    pub fn open(
        processes: &'a dyn ProcessRunner,
        root: &Path,
        remote: &str,
        reference: &str,
    ) -> Result<Self, String> {
        generation(reference)?;
        let common = code_git(
            processes,
            root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let hooks = code_git(
            processes,
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", "hooks"],
        )?;
        let format = code_git(processes, root, &["rev-parse", "--show-object-format"])?;
        let mut publisher = Self {
            processes,
            root: root.into(),
            staging: tempfile::tempdir().map_err(|_| "cannot create a private Git index")?,
            hooks: text(&hooks)?.into(),
            remote: String::new(),
            reference: format!("refs/heads/{reference}"),
            parent: None,
            entries: BTreeMap::new(),
        };
        // Preserve repository-local author, credential and hook configuration, but
        // isolate objects, refs, FETCH_HEAD and shallow state as well as the index.
        let output = publisher
            .processes
            .run(
                &publisher
                    .code(&["init", "--bare", "--quiet"])
                    .argument(format!("--object-format={}", text(&format)?))
                    .argument(publisher.staging.path().join("git").to_string_lossy()),
            )
            .map_err(|_| "cannot start Git publication")?;
        checked(output, "private repository initialization")?;
        let output = publisher
            .processes
            .run(
                &publisher
                    .code(&["config", "--file"])
                    .argument(
                        publisher
                            .staging
                            .path()
                            .join("git/config")
                            .to_string_lossy(),
                    )
                    .arguments(["--add", "include.path"])
                    .argument(Path::new(text(&common)?).join("config").to_string_lossy()),
            )
            .map_err(|_| "cannot configure private Git repository")?;
        checked(output, "private repository configuration")?;
        publisher.remote = publisher.remote(remote)?;
        publisher.parent = publisher.fetch_ref(&publisher.reference)?;
        if let Some(parent) = publisher.parent.as_deref() {
            publisher.entries = publisher.snapshot(parent)?;
            if publisher.read(MARKER_PATH)?.as_deref() != Some(MARKER) {
                return Err(format!("storage reference is not an initialized reviewed-artifact snapshot; {MIGRATION}"));
            }
            publisher.run(&["read-tree", parent], "private index initialization")?;
        } else {
            publisher.run(&["read-tree", "--empty"], "private index initialization")?;
        }
        publisher.put_bytes(MARKER_PATH, MARKER, false)?;
        Ok(publisher)
    }

    /// Starts a publication command against the private index and object store.
    fn command(&self, arguments: &[&str]) -> ProcessRequest {
        ProcessRequest::new("git")
            .without_variables(CLEAN_GIT)
            .with_variable("GIT_TERMINAL_PROMPT", "0")
            .with_variable("GIT_DIR", self.staging.path().join("git").to_string_lossy())
            .with_variable("GIT_WORK_TREE", self.root.to_string_lossy())
            .with_variable(
                "GIT_INDEX_FILE",
                self.staging.path().join("index").to_string_lossy(),
            )
            .argument("-C")
            .argument(self.root.to_string_lossy())
            .argument("-c")
            .argument(format!("core.hooksPath={}", self.hooks.display()))
            .arguments(arguments.iter().copied())
    }

    /// Starts a code-repository command with only the isolation applied.
    fn code(&self, arguments: &[&str]) -> ProcessRequest {
        ProcessRequest::new("git")
            .without_variables(CLEAN_GIT)
            .with_variable("GIT_TERMINAL_PROMPT", "0")
            .argument("-C")
            .argument(self.root.to_string_lossy())
            .arguments(arguments.iter().copied())
    }

    fn output(&self, arguments: &[&str]) -> Result<ProcessOutput, String> {
        self.processes
            .run(&self.command(arguments))
            .map_err(|_| "cannot start Git publication".into())
    }

    pub fn run(&self, arguments: &[&str], operation: &str) -> Result<Vec<u8>, String> {
        checked(self.output(arguments)?, operation)
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
        let source = text(&bytes)?;
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

    pub(super) fn fetch_ref(&self, reference: &str) -> Result<Option<String>, String> {
        let output = self.output(&[
            "ls-remote",
            "--exit-code",
            "--heads",
            "--",
            &self.remote,
            reference,
        ])?;
        if output.code == Some(2) {
            return Ok(None);
        }
        if !output.success {
            return Err(
                "cannot inspect storage remote; a failed lookup is not an empty branch".into(),
            );
        }
        self.run(
            &[
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-recurse-submodules",
                "--update-shallow",
                "--",
                &self.remote,
                reference,
            ],
            "complete storage history fetch",
        )?;
        if text(&self.run(
            &["rev-parse", "--is-shallow-repository"],
            "history completeness inspection",
        )?)? != "false"
        {
            return Err("storage history is shallow; cumulative-history accounting requires a complete remote history".into());
        }
        Ok(Some(object_id(&self.run(
            &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
            "storage commit lookup",
        )?)?))
    }

    pub(super) fn read_at(&self, commit: &str, path: &str) -> Result<Vec<u8>, String> {
        let object = format!("{commit}:{path}");
        let size = self.run(&["cat-file", "-s", &object], "storage object inspection")?;
        if text(&size)?
            .parse::<u64>()
            .ok()
            .is_none_or(|size| size > MAX_MANIFEST_BYTES)
        {
            return Err("storage metadata exceeds 4 MiB".into());
        }
        self.run(&["show", &object], "storage metadata retrieval")
    }

    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
        let Some(blob) = self.entries.get(path) else {
            return Ok(None);
        };
        if blob.size > MAX_MANIFEST_BYTES {
            return Err("storage metadata exceeds 4 MiB".into());
        }
        self.run(
            &["cat-file", "blob", &blob.object],
            "storage metadata retrieval",
        )
        .map(Some)
    }

    pub(super) fn verify_cas(&self, digest: &Sha256Digest) -> Result<(), String> {
        let path = format!("cas/{}", digest.sharded_path());
        let blob = self.entries.get(&path).ok_or("retained closure is missing an original/input CAS blob; re-ingest the reviewed original before migration")?;
        let temporary = self.staging.path().join("retained-original");
        let output = self
            .processes
            .run(
                &self
                    .command(&["cat-file", "blob", &blob.object])
                    .writing_stdout_to(&temporary),
            )
            .map_err(|_| "cannot start retained original verification")?;
        checked(output, "retained original retrieval")?;
        if isolock_cas::digest_file(&temporary).map_err(|error| error.to_string())? != *digest {
            return Err(
                "retained original/input SHA-256 mismatch; refusing epoch migration".into(),
            );
        }
        Ok(())
    }

    pub(super) fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, String> {
        let output = self.output(&["merge-base", "--is-ancestor", ancestor, descendant])?;
        match output.code {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err("cannot verify retained storage ancestry".into()),
        }
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
        let object = object_id(&self.run(
            &["hash-object", "-w", "--no-filters", "--", source],
            "blob staging",
        )?)?;
        if self
            .entries
            .get(path)
            .is_some_and(|existing| existing.object != object)
            && !mutable
        {
            return Err(
                "refusing to change an immutable published artifact or review record".into(),
            );
        }
        self.stage(
            path,
            Blob {
                object,
                size: metadata.len(),
            },
        )
    }

    fn stage(&mut self, path: &str, blob: Blob) -> Result<(), String> {
        self.run(
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "100644",
                &blob.object,
                path,
            ],
            "private index staging",
        )?;
        self.entries.insert(path.into(), blob);
        Ok(())
    }

    pub(super) fn retain(&mut self, paths: &BTreeSet<String>) -> Result<(), String> {
        let entries = std::mem::take(&mut self.entries);
        self.run(&["read-tree", "--empty"], "new orphan index initialization")?;
        for path in paths {
            self.stage(
                path,
                entries
                    .get(path)
                    .ok_or("retained snapshot path is missing")?
                    .clone(),
            )?;
        }
        Ok(())
    }

    pub fn pinned_source(&self, source: &str, commit: &str) -> Result<(), String> {
        let commit = object_id(commit.as_bytes())?;
        if self.local_or_https(source)?.as_deref() != Some(&self.remote) {
            return Err("selected profile is locked to another storage source".into());
        }
        self.run(
            &["fetch", "--quiet", "--no-tags", "--", &self.remote, &commit],
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
        let expected = object_id(&self.run(
            &["hash-object", "--no-filters", "--", source],
            "payload verification",
        )?)?;
        self.verify_object(commit, relative, &expected)
    }

    pub(super) fn verify_object(
        &self,
        commit: &str,
        path: &str,
        expected: &str,
    ) -> Result<(), String> {
        let actual = object_id(&self.run(
            &["rev-parse", "--verify", &format!("{commit}:{path}")],
            "locked payload lookup (publish reviewed originals before locking a profile)",
        )?)?;
        if actual != expected {
            return Err(
                "locked storage commit does not contain the selected verified payload bytes".into(),
            );
        }
        Ok(())
    }

    pub(super) fn tree(&self) -> Result<String, String> {
        object_id(&self.run(&["write-tree"], "storage tree creation")?)
    }

    pub(super) fn snapshot(&self, tree: &str) -> Result<BTreeMap<String, Blob>, String> {
        parse_tree(&self.run(&["ls-tree", "-rlz", tree], "storage tree inspection")?)
    }

    pub(super) fn live_bytes(&self) -> u64 {
        self.entries.values().map(|blob| blob.size).sum()
    }

    pub(super) fn history_bytes(&self) -> Result<u64, String> {
        let mut blobs: BTreeMap<String, u64> = self
            .entries
            .values()
            .map(|blob| (blob.object.clone(), blob.size))
            .collect();
        if let Some(parent) = &self.parent {
            let names = self.run(
                &["rev-list", "--objects", "--no-object-names", parent],
                "complete history enumeration",
            )?;
            let input = self.staging.path().join("history-objects");
            fs::write(&input, names).map_err(|_| "cannot stage history accounting")?;
            let output = self
                .processes
                .run(
                    &self
                        .command(&[
                            "cat-file",
                            "--batch-check=%(objectname) %(objecttype) %(objectsize)",
                        ])
                        .reading_stdin_from(&input),
                )
                .map_err(|_| "cannot start history accounting")?;
            let objects = checked(output, "history blob accounting")?;
            for line in text(&objects)?.lines() {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                if fields.len() != 3 {
                    return Err(
                        "incomplete storage history; cannot account for reachable objects".into(),
                    );
                }
                if fields[1] == "blob" {
                    blobs.insert(
                        object_id(fields[0].as_bytes())?,
                        fields[2].parse().map_err(|_| "invalid history blob size")?,
                    );
                }
            }
        }
        blobs.values().try_fold(0_u64, |total, size| {
            total
                .checked_add(*size)
                .ok_or_else(|| "history byte count overflow".into())
        })
    }

    pub(super) fn commit(
        &self,
        tree: &str,
        parent: Option<&str>,
        message: &str,
    ) -> Result<String, String> {
        let mut arguments = vec!["commit-tree", tree];
        if let Some(parent) = parent {
            arguments.extend(["-p", parent]);
        }
        arguments.extend(["-m", message]);
        object_id(&self.run(&arguments, "audit commit creation")?)
    }

    pub(super) fn atomic_epoch(
        &self,
        seal: &str,
        target: &str,
        reference: &str,
    ) -> Result<(), String> {
        self.run(&["push", "--porcelain", "--atomic", "--", &self.remote, &format!("{seal}:{}", self.reference), &format!("{target}:refs/heads/{reference}")],
            "atomic non-force epoch publication (atomic support required; outcome may be unknown after connection failure; inspect refs and retry the same reviewed policy)")?;
        Ok(())
    }

    pub fn finish(self, message: &str) -> Result<String, String> {
        if self.live_bytes() > MAX_TREE_BYTES {
            return Err("storage snapshot exceeds publication limits; use directory transport or an explicitly reviewed new epoch".into());
        }
        let tree = self.tree()?;
        if let Some(parent) = self.parent.as_deref() {
            if object_id(&self.run(
                &["rev-parse", &format!("{parent}^{{tree}}")],
                "parent tree lookup",
            )?)? == tree
            {
                return Ok(format!("Storage is up to date: {parent}"));
            }
        }
        let commit = self.commit(&tree, self.parent.as_deref(), message)?;
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

fn checked(output: ProcessOutput, operation: &str) -> Result<Vec<u8>, String> {
    if !output.success {
        return Err(format!("Git {operation} failed; check repository access, author configuration, and storage history"));
    }
    Ok(output.stdout)
}

fn code_git(
    processes: &dyn ProcessRunner,
    root: &Path,
    arguments: &[&str],
) -> Result<Vec<u8>, String> {
    checked(
        processes
            .run(
                &ProcessRequest::new("git")
                    .without_variables(CLEAN_GIT)
                    .with_variable("GIT_TERMINAL_PROMPT", "0")
                    .argument("-C")
                    .argument(root.to_string_lossy())
                    .arguments(arguments.iter().copied()),
            )
            .map_err(|_| "cannot start Git publication")?,
        "repository discovery",
    )
}

pub(super) fn text(bytes: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(bytes)
        .map(str::trim)
        .map_err(|_| "Git metadata is not UTF-8".into())
}

pub(super) fn object_id(bytes: &[u8]) -> Result<String, String> {
    let value = text(bytes)?;
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("invalid Git object ID".into());
    }
    Ok(value.into())
}

pub(super) fn generation(reference: &str) -> Result<u64, String> {
    let generation = reference.strip_prefix("isolock/v").unwrap_or_default();
    if generation.starts_with('0')
        || generation.is_empty()
        || !generation.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("publication requires a storage reference isolock/vN; code branches and tags are forbidden".into());
    }
    generation
        .parse()
        .map_err(|_| "storage generation exceeds the supported integer range".into())
}

fn parse_tree(bytes: &[u8]) -> Result<BTreeMap<String, Blob>, String> {
    let mut entries = BTreeMap::new();
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let entry = std::str::from_utf8(entry).map_err(|_| "storage tree is not UTF-8")?;
        let (metadata, path) = entry.split_once('\t').ok_or("invalid storage tree entry")?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 4 || fields[0] != "100644" || fields[1] != "blob" || !allowed_path(path)
        {
            return Err(format!(
                "storage reference contains non-snapshot paths or unsafe modes; {MIGRATION}"
            ));
        }
        let size = fields[3]
            .parse::<u64>()
            .map_err(|_| "invalid storage blob size")?;
        if size > SHARD_SIZE_BYTES {
            return Err(format!(
                "storage snapshot exceeds publication limits; {MIGRATION}"
            ));
        }
        entries.insert(
            path.into(),
            Blob {
                object: object_id(fields[2].as_bytes())?,
                size,
            },
        );
    }
    Ok(entries)
}

fn allowed_path(path: &str) -> bool {
    if matches!(path, MARKER_PATH | "plan.json" | "epoch.json") {
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
