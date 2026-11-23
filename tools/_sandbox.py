"""Developer sandbox provisioning and publication commands."""

from __future__ import annotations

import argparse
import os
import shutil
import tempfile
import urllib.request
from pathlib import Path

from _shared import fail, log, read_sums, run, sha256_file

SANDBOX_ASSET = "pixi-sandbox-x86_64-unknown-linux-musl"


def _download(url: str, destination: Path) -> None:
    log(f"download {url}")
    with urllib.request.urlopen(url, timeout=120) as response, destination.open("wb") as handle:
        shutil.copyfileobj(response, handle)


def fetch_binary(args: argparse.Namespace) -> None:
    """Fetch and checksum-verify the pinned pixi-sandbox release binary."""
    destination = Path(args.output)
    base = (
        "https://github.com/Archont561/pixi-sandbox/releases/download/"
        f"v{args.version}"
    )
    with tempfile.TemporaryDirectory(prefix="pixi-sandbox-fetch.") as scratch:
        stage = Path(scratch)
        asset = stage / SANDBOX_ASSET
        manifest = stage / "SHA256SUMS"
        _download(f"{base}/{SANDBOX_ASSET}", asset)
        _download(f"{base}/SHA256SUMS", manifest)
        digests = [digest for digest, name in read_sums(manifest) if name in (SANDBOX_ASSET, f"*{SANDBOX_ASSET}")]
        if len(digests) != 1 or len(digests[0]) != 64:
            fail(f"invalid or ambiguous checksum for {SANDBOX_ASSET}")
        actual = sha256_file(asset)
        if actual != digests[0]:
            fail(f"checksum mismatch for {SANDBOX_ASSET}: {actual} != {digests[0]}")
        asset.chmod(0o755)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(asset), destination)
    log(f"verified {SANDBOX_ASSET} -> {destination}")


def publish(args: argparse.Namespace) -> None:
    """Pack, verify, and publish the locked developer sandbox branch."""
    binary = os.environ.get("PIXI_SANDBOX_BIN")
    if not binary:
        fail("missing required environment variable: PIXI_SANDBOX_BIN")
    run(
        [
            binary,
            "pipeline",
            "--envs",
            args.envs,
            "--platform",
            args.platform,
            "--branch",
            args.branch,
            "--remote",
            os.environ.get("REMOTE", ""),
            "--config",
            "pixi-sandbox.toml",
            "--cargo-vendor",
            "true",
            "--transport-dir",
            os.environ.get("PIXI_SANDBOX_TRANSPORT", str(Path(args.transport))),
            "--log-dir",
            os.environ.get("PIXI_SANDBOX_LOGS", str(Path(args.logs))),
        ]
    )


def register(groups: argparse._SubParsersAction) -> None:
    """Attach the sandbox commands to the root parser."""
    group = groups.add_parser("sandbox", help="Provision and publish the developer sandbox.")
    commands = group.add_subparsers(dest="command", required=True, metavar="COMMAND")

    fetch = commands.add_parser("fetch-binary", help="Fetch the pinned pixi-sandbox binary.")
    fetch.add_argument("--version", required=True, help="Released pixi-sandbox version.")
    fetch.add_argument("--output", required=True, help="Destination path for the binary.")
    fetch.set_defaults(handler=fetch_binary)

    pack = commands.add_parser("publish", help="Pack, verify, and publish the sandbox branch.")
    pack.add_argument("--envs", default="default", help="Comma-separated environment names.")
    pack.add_argument("--platform", default="linux-64", help="Sandbox platform to pack.")
    pack.add_argument("--branch", required=True, help="Transport branch to publish.")
    pack.add_argument("--transport", default="/tmp/pixi-sandbox-transport", help="Transport directory.")
    pack.add_argument("--logs", default="/tmp/pixi-sandbox-logs", help="Diagnostics log directory.")
    pack.set_defaults(handler=publish)
