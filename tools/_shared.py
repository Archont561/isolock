"""Shared process and hashing helpers for the repository tool CLI."""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys
from pathlib import Path
from typing import Iterable, Mapping, NoReturn, Sequence

REPO = Path(__file__).resolve().parent.parent

_CHUNK = 1 << 20


def log(message: str) -> None:
    """Print a tool progress line to stderr."""
    print(message, file=sys.stderr, flush=True)


def fail(message: str) -> NoReturn:
    """Abort with a single actionable error line."""
    raise SystemExit(message)


def merged_env(overrides: Mapping[str, str] | None = None) -> dict[str, str]:
    """Return the current environment with explicit overrides applied."""
    env = dict(os.environ)
    if overrides:
        env.update({key: str(value) for key, value in overrides.items()})
    return env


def run(
    argv: Sequence[str],
    *,
    cwd: Path | str | None = None,
    env: Mapping[str, str] | None = None,
) -> None:
    """Run a foreground command in the current workspace, echoing its argv."""
    printable = " ".join(str(part) for part in argv)
    log(f"+ {printable}")
    subprocess.run(list(argv), cwd=str(cwd or Path.cwd()), env=env, check=True)


def capture(argv: Sequence[str], *, cwd: Path | str | None = None) -> str:
    """Run a command and return its stripped standard output."""
    result = subprocess.run(
        list(argv),
        cwd=str(cwd or Path.cwd()),
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    return result.stdout.strip()


def sha256_file(path: Path) -> str:
    """Stream a file through SHA-256 without loading it into memory."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(_CHUNK), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_sums(paths: Iterable[Path], destination: Path) -> None:
    """Write a `sha256sum`-compatible manifest for the given files."""
    lines = [f"{sha256_file(path)}  {path.name}" for path in paths]
    destination.write_text("\n".join(lines) + "\n", encoding="utf-8")


def read_sums(manifest: Path) -> list[tuple[str, str]]:
    """Parse a `sha256sum`-compatible manifest into digest/name pairs."""
    entries: list[tuple[str, str]] = []
    for line in manifest.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        digest, _, name = line.partition(" ")
        entries.append((digest, name.strip().lstrip("*")))
    return entries


def verify_sums(directory: Path) -> None:
    """Verify every entry of a directory's SHA256SUMS manifest."""
    manifest = directory / "SHA256SUMS"
    if not manifest.is_file():
        fail(f"missing checksum manifest: {manifest}")
    for digest, name in read_sums(manifest):
        target = directory / name
        if not target.is_file():
            fail(f"checksum manifest names a missing file: {target}")
        actual = sha256_file(target)
        if actual != digest:
            fail(f"checksum mismatch for {target}: {actual} != {digest}")


def workspace_version() -> str:
    """Read the workspace package version from the root Cargo.toml."""
    in_package = False
    for line in (REPO / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            in_package = stripped == "[workspace.package]"
            continue
        if in_package and stripped.startswith("version") and "=" in stripped:
            return stripped.split("=", 1)[1].strip().strip('"')
    fail("cannot determine the workspace package version from Cargo.toml")
