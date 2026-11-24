//! Commit-tree acquisition for Cargo git dependencies.
//!
//! A Cargo git dependency has no `checksum` in `Cargo.lock`, so the reviewed
//! pin is the SHA-256 of `git archive` output for one resolved commit. That is
//! the mechanism the design note settled on: the digest identifies bytes we
//! hold, and the commit SHA is provenance for a reviewer.
//!
//! `git archive` output is reproducible. It stamps every entry with the
//! commit's own timestamp, not the wall clock, so re-exporting the same commit
//! on any machine and any day yields the same bytes. That is what makes a
//! digest reviewable before ingestion: a reviewer who trusts a commit SHA can
//! predict the pin.
//!
//! Fetching is a shallow, single-commit `fetch` into a disposable bare
//! repository, mirroring `infrastructure::transport`. The user's checkout and
//! `CARGO_HOME` are never touched.

use crate::application::ports::{ProcessRequest, ProcessRunner};
use isolock_cas::{digest_file, FileStore, Sha256Digest};
use std::fs;
use std::path::{Path, PathBuf};

/// Variables that would redirect Git away from the repository this module owns.
const CLEAN_GIT: [&str; 7] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_SHALLOW_FILE",
];

/// One reviewed commit export: where it came from, what it resolved to, the bytes, and their digest.
#[derive(Debug)]
pub struct Exported {
    /// The credential-free repository URL, as reviewed.
    pub repository: String,
    /// The full commit the reviewed revision resolved to.
    pub commit: String,
    /// The reviewed revision as supplied, which may be a tag, branch, or prefix.
    pub requested: String,
    /// SHA-256 of the exported archive, which becomes the artifact digest.
    pub digest: Sha256Digest,
    /// The exported archive basename, safe to record in a manifest.
    pub filename: String,
    /// The exported archive, retained inside the staging directory.
    pub archive: PathBuf,
}

impl Exported {
    /// The archive filename recorded in the reviewed manifest.
    ///
    /// Derived from the reviewed crate name, not the resolved commit, so the
    /// filename a reviewer writes in the manifest is known before ingestion.
    /// Commit identity travels in `metadata`, not in the path.
    #[must_use]
    pub fn filename(name: &str) -> String {
        format!("{name}-git.tar")
    }
}

/// Fetches `repository`, exports the resolved commit, and digests the archive.
///
/// `name` is the reviewed crate name and only shapes the archive basename. The
/// returned [`Exported`] carries everything the manifest record needs.
///
/// # Errors
/// Returns a message when the source or revision is unsafe, the fetch or export
/// fails, or the resolved commit is not a full SHA.
pub fn export(
    processes: &dyn ProcessRunner,
    staging: &Path,
    name: &str,
    repository: &str,
    revision: &str,
) -> Result<Exported, ValidationError> {
    validate(repository, revision).map_err(ValidationError)?;
    isolock_plan::profile::filename(&Exported::filename(name)).map_err(ValidationError)?;
    let bare = staging.join("git-import");
    if bare.exists() {
        fs::remove_dir_all(&bare).map_err(|error| ValidationError(error.to_string()))?;
    }
    fs::create_dir_all(&bare).map_err(|error| ValidationError(error.to_string()))?;
    git(processes, &bare, &["init", "--bare", "--quiet"])?;
    // Shallow, single-commit, no tags: this repository exists only long enough
    // to export one tree, so it must not accumulate history.
    git(
        processes,
        &bare,
        &[
            "fetch",
            "--quiet",
            "--depth=1",
            "--no-tags",
            "--",
            repository,
            revision,
        ],
    )?;
    let commit = resolved(processes, &bare)?;
    let filename = Exported::filename(name);
    let archive = staging.join(&filename);
    git(
        processes,
        &bare,
        &[
            "archive",
            "--format=tar",
            "--output",
            archive
                .to_str()
                .ok_or_else(|| ValidationError("staging path is not UTF-8".into()))?,
            &commit,
        ],
    )?;
    let digest = digest_file(&archive).map_err(|error| ValidationError(error.to_string()))?;
    Ok(Exported {
        repository: repository.to_owned(),
        commit,
        requested: revision.to_owned(),
        digest,
        filename,
        archive,
    })
}

/// Verifies the exported archive against its reviewed pin and stores it in CAS.
///
/// The pin, not the commit, is what the CAS entry is keyed by. A commit SHA is
/// not a hash of the bytes we ship, so only this digest can detect a changed
/// tree.
///
/// # Errors
/// Returns a message when the archive is missing, is not a regular file,
/// changed after review, or does not match the reviewed digest.
pub fn store(
    store: &FileStore,
    reviewed: &Sha256Digest,
    exported: &Exported,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&exported.archive)
        .map_err(|_| "cannot read exported commit archive")?;
    if !metadata.file_type().is_file() {
        return Err("exported commit archive must be a regular file".into());
    }
    if digest_file(&exported.archive).map_err(|error| error.to_string())? != *reviewed
        || exported.digest != *reviewed
    {
        return Err("reviewed Cargo git dependency does not match its pinned digest".into());
    }
    store
        .put_file(reviewed, &exported.archive)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Rejects a source or revision that could steer `git` somewhere unintended.
///
/// This mirrors `infrastructure::transport`: a leading `-` would be parsed as an
/// option, and a revision is interpolated into a fetch refspec, so control
/// characters and the refspec separator are refused before any process starts.
pub fn validate(repository: &str, revision: &str) -> Result<(), String> {
    if !(repository.starts_with("https://") || repository.starts_with("file://")) {
        return Err("Cargo git dependency sources must be public HTTPS URLs".into());
    }
    isolock_plan::profile::public_source(repository)?;
    if repository.starts_with('-')
        || revision.starts_with('-')
        || revision.is_empty()
        || revision.contains(['\0', '\n', '\r', ':', '\\', ' '])
    {
        return Err("invalid Cargo git dependency revision".into());
    }
    if repository.contains(['\\', ' '])
        || repository
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("invalid Cargo git dependency source".into());
    }
    Ok(())
}

/// A reviewed-input rejection, distinguishable from an operational failure.
#[derive(Debug)]
pub struct ValidationError(pub String);

impl From<ValidationError> for String {
    fn from(error: ValidationError) -> Self {
        error.0
    }
}

/// Runs one `git` command inside `repository`, returning stdout.
///
/// Fetch failures and export failures need different operator messages, so the
/// operation is named by the caller rather than derived here.
fn git(
    processes: &dyn ProcessRunner,
    repository: &Path,
    arguments: &[&str],
) -> Result<Vec<u8>, ValidationError> {
    let output = processes
        .run(
            &ProcessRequest::new("git")
                .without_variables(CLEAN_GIT)
                .with_variable("GIT_TERMINAL_PROMPT", "0")
                .argument("-C")
                .argument(repository.to_string_lossy())
                .arguments(arguments.iter().copied()),
        )
        .map_err(|_| ValidationError("cannot start Git for Cargo git dependency export".into()))?;
    if !output.success {
        return Err(ValidationError(format!(
            "Cargo git dependency {} failed; check the reviewed source and revision",
            arguments.first().copied().unwrap_or("command")
        )));
    }
    Ok(output.stdout)
}

/// The full commit `FETCH_HEAD` resolved to, refusing anything but a full lowercase SHA.
fn resolved(processes: &dyn ProcessRunner, repository: &Path) -> Result<String, ValidationError> {
    let bytes = git(
        processes,
        repository,
        &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
    )?;
    let commit = String::from_utf8(bytes)
        .map_err(|_| ValidationError("Cargo git dependency commit is not UTF-8".into()))?;
    let commit = commit.trim().to_owned();
    if !matches!(commit.len(), 40 | 64)
        || !commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ValidationError(
            "Cargo git dependency did not resolve to a full commit".into(),
        ));
    }
    Ok(commit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::fakes::FakeProcesses;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn unsafe_sources_and_revisions_are_rejected_before_any_process() {
        for (repository, revision) in [
            ("-oProxyCommand=x", COMMIT),
            ("https://example.test/a.git", "--upload-pack=x"),
            ("https://example.test/a.git", ""),
            ("https://example.test/a.git", "main:refs/heads/x"),
            ("https://user:token@example.test/a.git", COMMIT),
            ("http://example.test/a.git", COMMIT),
            ("https://example.test/a b.git", COMMIT),
            ("https://example.test/a.git", "rev\n"),
        ] {
            assert!(
                validate(repository, revision).is_err(),
                "{repository} {revision}"
            );
        }
    }

    #[test]
    fn public_https_and_file_sources_are_accepted() {
        assert!(validate("https://example.test/org/lib.git", COMMIT).is_ok());
        assert!(validate("https://example.test/org/lib.git", "v1.2.3").is_ok());
        assert!(validate("file:///tmp/lib", COMMIT).is_ok());
    }

    #[test]
    fn a_revision_that_does_not_resolve_to_a_full_commit_is_rejected() {
        for stdout in [
            b"abc\n".to_vec(),
            COMMIT.to_uppercase().into_bytes(),
            b"0123456789abcdef0123456789abcdef0123456g\n".to_vec(),
            b"0123456789abcdef0123456789abcdef01234567 extra\n".to_vec(),
        ] {
            let fake = FakeProcesses::new().printing(stdout);
            let error = resolved(&fake, Path::new("/tmp")).unwrap_err();
            assert!(error.0.contains("full commit"), "{}", error.0);
        }
    }

    #[test]
    fn fetch_is_shallow_single_commit_and_never_prompting() {
        let fake = FakeProcesses::new()
            .succeeding()
            .succeeding()
            .printing(format!("{COMMIT}\n"))
            .succeeding();
        let staging = tempfile::tempdir().unwrap();
        let exported = export(
            &fake,
            staging.path(),
            "lib",
            "https://example.test/org/lib.git",
            "v1.2.3",
        )
        .unwrap_err();
        // The scripted `git archive` wrote no file, so digesting fails closed.
        assert!(
            exported.0.contains("No such file") || exported.0.contains("cannot"),
            "{}",
            exported.0
        );
        let invocations = fake.invocations();
        assert_eq!(invocations.len(), 4);
        assert!(invocations[0]
            .arguments()
            .ends_with("git-import init --bare --quiet"));
        assert_eq!(
            invocations[1].arguments(),
            format!(
                "-C {} fetch --quiet --depth=1 --no-tags -- https://example.test/org/lib.git v1.2.3",
                staging.path().join("git-import").display()
            )
        );
        assert_eq!(
            invocations[1].environment("GIT_TERMINAL_PROMPT").as_deref(),
            Some("0")
        );
        assert!(invocations[1].removed("GIT_DIR"));
        assert_eq!(
            invocations[2].arguments(),
            format!(
                "-C {} rev-parse --verify FETCH_HEAD^{{commit}}",
                staging.path().join("git-import").display()
            )
        );
        assert_eq!(
            invocations[3].arguments(),
            format!(
                "-C {} archive --format=tar --output {} {COMMIT}",
                staging.path().join("git-import").display(),
                staging.path().join("lib-git.tar").display()
            )
        );
    }

    #[test]
    fn the_archive_name_is_a_safe_basename() {
        assert_eq!(Exported::filename("lib"), "lib-git.tar");
        assert!(isolock_plan::profile::filename(&Exported::filename("lib")).is_ok());
        assert!(isolock_plan::profile::filename(&Exported::filename("../x")).is_err());
    }
}
