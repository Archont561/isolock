"""Release packaging, verification, and publication commands."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path

from _shared import capture, fail, log, read_sums, run, sha256_file, verify_sums, workspace_version, write_sums

PLATFORM_COUNT = 3


def extract(archive: Path, destination: Path) -> None:
    """Extract a tar archive, using the safe data filter when available."""
    with tarfile.open(archive) as handle:
        try:
            handle.extractall(destination, filter="data")
        except TypeError:  # Python < 3.12 has no extraction filters
            handle.extractall(destination)


def dist() -> Path:
    """Return the release staging root inside the current workspace."""
    return Path.cwd() / "dist"


def build_binary(args: argparse.Namespace) -> None:
    """Build the standalone CLI for one explicit target triple."""
    run(
        [
            "cargo",
            "build",
            "--release",
            "--locked",
            "--package",
            "isolock",
            "--target",
            args.target,
        ]
    )


def install_rattler_build(_: argparse.Namespace) -> None:
    """Provision the pinned Conda packaging tool through Pixi global."""
    run(["pixi", "global", "install", "rattler-build"])


def check_tag(_: argparse.Namespace) -> None:
    """Fail unless a publishing dispatch names an explicit v* tag."""
    publish = os.environ.get("RELEASE_PUBLISH", "").lower() == "true"
    if not publish:
        log("rehearsal run: publication is disabled for this dispatch")
        return
    tag = os.environ.get("RELEASE_TAG", "")
    if not tag.startswith("v"):
        fail(f"publishing requires an explicit v* tag, got {tag!r}")


def build_conda(args: argparse.Namespace) -> None:
    """Build the Conda package for one target platform."""
    (dist() / "conda").mkdir(parents=True, exist_ok=True)
    argv = [
        "rattler-build",
        "build",
        "--recipe",
        "crates/isolock/recipe.yaml",
        "--output-dir",
        "dist/conda",
    ]
    if args.platform:
        argv[4:4] = ["--target-platform", args.platform]
    run(argv)


def checksum_conda(_: argparse.Namespace) -> None:
    """Write a single-package SHA256SUMS manifest for the built Conda archive."""
    root = dist() / "conda"
    packages = sorted(root.rglob("*.conda"))
    if len(packages) != 1:
        fail(f"expected one Conda package, found {len(packages)}")
    relative = packages[0].relative_to(root).as_posix()
    line = f"{sha256_file(packages[0])}  {relative}\n"
    (root / "SHA256SUMS").write_text(line, encoding="utf-8")
    log(line.rstrip())


def stage_binaries(args: argparse.Namespace) -> None:
    """Copy the built binary, archive it, and checksum the binary artifacts."""
    binaries = dist() / "binaries"
    binaries.mkdir(parents=True, exist_ok=True)
    release = Path.cwd() / "target" / args.target / "release"
    source = release / args.binary
    if not source.is_file():
        fail(f"missing built binary: {source}")
    staged = binaries / f"isolock-{args.platform}{source.suffix}"
    shutil.copy2(source, staged)
    if source.suffix == "":
        staged.chmod(0o755)
    with tarfile.open(binaries / args.archive_name, "w:gz") as archive:
        archive.add(source, arcname=args.binary)
    write_sums(sorted(path for path in binaries.iterdir() if path.name != "SHA256SUMS"), binaries / "SHA256SUMS")


def verify_binary(args: argparse.Namespace) -> None:
    """Verify the standalone artifact checksums and smoke-test the extraction."""
    artifacts = Path(args.directory).resolve()
    verify_sums(artifacts)
    extracted = artifacts / "extracted"
    extracted.mkdir(parents=True, exist_ok=True)
    extract(artifacts / args.archive_name, extracted)
    smoke(None, extracted / args.binary)


def assemble_candidate(_: argparse.Namespace) -> None:
    """Verify every artifact set and assemble the unique release asset bundle."""
    binaries = sorted((dist() / "binaries").glob("binary-*"))
    conda_sets = sorted((dist() / "conda").glob("conda-*"))
    if len(binaries) != PLATFORM_COUNT or len(conda_sets) != PLATFORM_COUNT:
        fail(
            f"expected {PLATFORM_COUNT} binary and {PLATFORM_COUNT} Conda artifact sets; "
            f"got {len(binaries)} and {len(conda_sets)}"
        )
    release = dist() / "release"
    release.mkdir(parents=True, exist_ok=True)
    packages = 0
    for artifact_set in [*binaries, *conda_sets]:
        verify_sums(artifact_set)
        for _, name in read_sums(artifact_set / "SHA256SUMS"):
            source = artifact_set / name
            if source.suffix == ".conda":
                with zipfile.ZipFile(source) as archive:
                    if archive.testzip() is not None:
                        fail(f"corrupt Conda archive: {source}")
                packages += 1
            destination = release / source.name
            if destination.exists():
                fail(f"duplicate release asset basename: {source.name}")
            shutil.copy2(source, destination)
    archives = sorted(release.glob("*.tar.gz"))
    if len(archives) != PLATFORM_COUNT or packages != PLATFORM_COUNT:
        fail(
            f"expected {PLATFORM_COUNT} standalone archives and {PLATFORM_COUNT} Conda packages; "
            f"got {len(archives)} and {packages}"
        )
    write_sums(sorted(path for path in release.iterdir() if path.name != "SHA256SUMS"), release / "SHA256SUMS")
    verify_sums(release)


def verify_oidc(_: argparse.Namespace) -> None:
    """Fail fast when the publishing job lacks GitHub OIDC token permissions."""
    missing = [
        name
        for name in ("ACTIONS_ID_TOKEN_REQUEST_URL", "ACTIONS_ID_TOKEN_REQUEST_TOKEN")
        if not os.environ.get(name)
    ]
    if missing:
        fail(
            "missing GitHub OIDC request variables "
            f"({', '.join(missing)}); grant permissions.id-token: write"
        )


def publish_conda(args: argparse.Namespace) -> None:
    """Upload the verified Conda packages with a trusted-publishing attestation."""
    packages = sorted(path for path in (dist() / "conda").rglob("*.conda"))
    if len(packages) != PLATFORM_COUNT:
        fail(f"expected {PLATFORM_COUNT} Conda packages to publish, found {len(packages)}")
    run(
        [
            "rattler-build",
            "upload",
            "prefix",
            "--channel",
            args.channel,
            "--generate-attestation",
            "--store-github-attestation",
            "--skip-existing",
            *[str(path) for path in packages],
        ]
    )


def smoke(_: argparse.Namespace, binary_path: Path | None = None, args: argparse.Namespace | None = None) -> None:
    """Assert the standalone binary behaves like a released install."""
    if binary_path is None:
        if args is None:
            fail("release smoke requires a binary path")
        binary_path = Path(args.binary).resolve()
    if not binary_path.is_file():
        fail(f"release binary does not exist: {binary_path}")
    expected = f"isolock {workspace_version()}"
    version = capture([str(binary_path), "--version"])
    if version != expected:
        fail(f"unexpected release version: {version} (expected {expected})")
    with tempfile.TemporaryDirectory(prefix="isolock-release-smoke.") as scratch:
        root = Path(scratch)
        (root / "home").mkdir()
        (root / "config").mkdir()
        help_text = capture([str(binary_path), "--help"])
        if "Hermetic dependency airlock manager" not in help_text:
            fail("release binary help omits the project description")
        (root / "plan.json").write_text('{"artifacts":[]}', encoding="utf-8")
        env = {
            "HOME": str(root / "home"),
            "XDG_CONFIG_HOME": str(root / "config"),
            "ISOLOCK_USER_CONFIG": str(root / "no-user-config.toml"),
        }
        run([str(binary_path), "config", "check"], cwd=root, env={**os.environ, **env})
        verdict = subprocess.run(
            [str(binary_path), "plan", "--ci"],
            cwd=root,
            env={
                **os.environ,
                **env,
                "ISOLOCK_PLAN": str(root / "plan.json"),
                "ISOLOCK_CAS_DIR": str(root / "cas"),
            },
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        )
        if "Verdict: ALL DEPENDENCIES SATISFIED" not in verdict.stdout:
            fail("release binary did not pass a clean CI plan")
    log(f"release smoke passed: {version}")


def host_smoke(args: argparse.Namespace) -> None:
    """Package, checksum, extract, and smoke-test a natively built binary."""
    host = capture(["rustc", "-vV"]).splitlines()
    host_triple = next(line.split(": ", 1)[1] for line in host if line.startswith("host: "))
    if host_triple != args.target:
        fail(f"runner host {host_triple} cannot natively execute {args.target}")
    with tempfile.TemporaryDirectory(prefix="isolock-artifact-smoke.") as scratch:
        root = Path(scratch)
        stage, extracted = root / "stage", root / "extracted"
        stage.mkdir()
        extracted.mkdir()
        source = Path.cwd() / "target" / args.target / "release" / args.binary
        if not source.is_file():
            fail(f"missing built binary: {source}")
        shutil.copy2(source, stage / args.binary)
        archive = root / f"isolock-{args.platform}.tar.gz"
        with tarfile.open(archive, "w:gz") as handle:
            handle.add(stage / args.binary, arcname=args.binary)
        write_sums([archive], root / "SHA256SUMS")
        verify_sums(root)
        extract(archive, extracted)
        smoke(None, extracted / args.binary)


def register(groups: argparse._SubParsersAction) -> None:
    """Attach the release commands to the root parser."""
    group = groups.add_parser("release", help="Package, verify, and publish release assets.")
    commands = group.add_subparsers(dest="command", required=True, metavar="COMMAND")

    build = commands.add_parser("build-binary", help="Build the standalone CLI for a target.")
    build.add_argument("--target", required=True, help="Cargo target triple to build.")
    build.set_defaults(handler=build_binary)

    commands.add_parser(
        "install-rattler-build",
        help="Install rattler-build through Pixi global.",
    ).set_defaults(handler=install_rattler_build)

    tag = commands.add_parser("check-tag", help="Require an explicit v* tag for publication.")
    tag.set_defaults(handler=check_tag)

    conda = commands.add_parser("build-conda", help="Build the Conda package.")
    conda.add_argument("--platform", help="Target Conda subdir, for example linux-64.")
    conda.set_defaults(handler=build_conda)

    commands.add_parser("checksum-conda", help="Write the Conda SHA256SUMS manifest.").set_defaults(
        handler=checksum_conda
    )

    stage = commands.add_parser("stage-binaries", help="Archive and checksum the standalone binary.")
    stage.add_argument("--platform", required=True, help="Release platform label.")
    stage.add_argument("--binary", required=True, help="Binary file name to stage.")
    stage.add_argument("--archive-name", required=True, help="Archive file name to produce.")
    stage.add_argument("--target", help="Cargo target triple, when cross-compiled.")
    stage.set_defaults(handler=stage_binaries)

    verify = commands.add_parser("verify-binary", help="Verify and smoke-test a standalone archive.")
    verify.add_argument("--directory", default="dist/binaries", help="Directory holding the artifacts.")
    verify.add_argument("--archive-name", required=True, help="Archive file name to extract.")
    verify.add_argument("--binary", required=True, help="Binary file name inside the archive.")
    verify.set_defaults(handler=verify_binary)

    commands.add_parser(
        "assemble-candidate",
        help="Verify every artifact set and assemble the release candidate.",
    ).set_defaults(handler=assemble_candidate)

    commands.add_parser("verify-oidc", help="Check the OIDC token environment.").set_defaults(
        handler=verify_oidc
    )

    publish = commands.add_parser("publish-conda", help="Upload Conda packages to prefix.dev.")
    publish.add_argument("--channel", default="archont561/archont561", help="Target prefix.dev channel.")
    publish.set_defaults(handler=publish_conda)

    smoke_command = commands.add_parser("smoke", help="Smoke-test a standalone binary.")
    smoke_command.add_argument("--binary", required=True, help="Path to the binary under test.")
    smoke_command.set_defaults(handler=lambda args: smoke(None, args=args))

    host = commands.add_parser(
        "host-smoke",
        help="Package, checksum, extract, and smoke-test a native release binary.",
    )
    host.add_argument("--platform", required=True, help="Release platform label.")
    host.add_argument("--target", required=True, help="Required natively executable target triple.")
    host.add_argument("--binary", required=True, help="Binary file name to package.")
    host.set_defaults(handler=host_smoke)
