"""Airlock plan guard and reviewed-ingestion workflow commands."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

from _shared import fail, log, run

STORAGE_REF = "isolock/v1"


def _output(entries: dict[str, str]) -> None:
    target = os.environ.get("GITHUB_OUTPUT")
    if not target:
        fail("missing required environment variable: GITHUB_OUTPUT")
    with Path(target).open("a", encoding="utf-8") as handle:
        for key, value in entries.items():
            handle.write(f"{key}={value}\n")


def _notice(message: str) -> None:
    print(f"::notice::{message}", flush=True)


def _remote_refs(kind: str) -> str:
    result = subprocess.run(
        ["git", "ls-remote", f"--{kind}", "origin", f"refs/{kind}/{STORAGE_REF}"],
        check=False,
        stdout=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        fail(f"cannot read {STORAGE_REF} from origin; fetch origin refs before the guard runs")
    return result.stdout.strip()


def storage_ref(_: argparse.Namespace) -> None:
    """Resolve the published airlock storage requirement for the plan guard."""
    branch = _remote_refs("heads")
    tag = _remote_refs("tags")
    if branch:
        run(
            [
                "git",
                "fetch",
                "origin",
                f"refs/heads/{STORAGE_REF}:refs/remotes/origin/{STORAGE_REF}",
                "--depth=1",
            ]
        )
        _output({"available": "true", "reference": f"refs/remotes/origin/{STORAGE_REF}"})
        return
    if tag:
        run(["git", "fetch", "origin", f"refs/tags/{STORAGE_REF}:refs/tags/{STORAGE_REF}", "--depth=1"])
        _output({"available": "true", "reference": f"refs/tags/{STORAGE_REF}"})
        return
    _output({"available": "false"})
    _notice(f"No {STORAGE_REF} storage ref exists; skipping published snapshot verification.")


def audit_snapshot(_: argparse.Namespace) -> None:
    """Audit the published plan and CAS snapshot instead of producer caches."""
    binary = os.environ.get("ISOLOCK_BIN")
    reference = os.environ.get("ISOLOCK_SNAPSHOT_REF")
    if not binary or not reference:
        fail("ISOLOCK_BIN and ISOLOCK_SNAPSHOT_REF are required")
    runner_temp = Path(os.environ.get("RUNNER_TEMP", tempfile.gettempdir()))
    runner_temp.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="isolock-snapshot.", dir=runner_temp) as scratch:
        snapshot = Path(scratch)
        archive = subprocess.run(
            ["git", "archive", reference],
            check=True,
            stdout=subprocess.PIPE,
        )
        extract = subprocess.run(
            ["tar", "-x", "-C", str(snapshot)],
            input=archive.stdout,
            check=True,
        )
        if extract.returncode != 0:
            fail("cannot extract the published storage snapshot")
        log("Verifying the published plan and CAS, not solving project lockfiles.")
        subprocess.run(
            [binary, "plan", "--ci", "--cas-dir", "cas"],
            cwd=snapshot,
            env={**os.environ, "ISOLOCK_PLAN": "plan.json"},
            check=True,
        )


def ingest(_: argparse.Namespace) -> None:
    """Ingest and publish exactly the reviewed package selection."""
    binary = os.environ.get("ISOLOCK_BIN")
    manifest = os.environ.get("ISOLOCK_INGEST_MANIFEST")
    registry = os.environ.get("ISOLOCK_REGISTRY")
    packages = os.environ.get("ISOLOCK_PACKAGES", "").split()
    if not binary or not manifest or not registry:
        fail("ISOLOCK_BIN, ISOLOCK_INGEST_MANIFEST, and ISOLOCK_REGISTRY are required")
    if not packages:
        fail("an explicit reviewed package selection is required")
    subprocess.run(
        ["git", "ls-files", "--error-unmatch", "--", manifest],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    selection = ["--manifest", manifest, "--registry", registry]
    run([binary, "ingest", *selection, "--download", "--", *packages])
    run([binary, "push", *selection, "--", *packages])


def git_author(_: argparse.Namespace) -> None:
    """Configure the bot author used by publication commits."""
    run(["git", "config", "user.name", "github-actions[bot]"])
    run(["git", "config", "user.email", "github-actions[bot]@users.noreply.github.com"])


def register(groups: argparse._SubParsersAction) -> None:
    """Attach the airlock commands to the root parser."""
    group = groups.add_parser("airlock", help="Airlock plan guard and ingestion workflow steps.")
    commands = group.add_subparsers(dest="command", required=True, metavar="COMMAND")

    commands.add_parser("storage-ref", help="Resolve the published storage ref for the guard.").set_defaults(
        handler=storage_ref
    )
    commands.add_parser("audit-snapshot", help="Audit the published plan and CAS snapshot.").set_defaults(
        handler=audit_snapshot
    )
    commands.add_parser("ingest", help="Ingest and publish the reviewed selection.").set_defaults(
        handler=ingest
    )
    commands.add_parser("git-author", help="Configure the publication bot author.").set_defaults(
        handler=git_author
    )
