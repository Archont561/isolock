"""Documentation site build and Pages availability commands."""

from __future__ import annotations

import argparse
import json
import os
import urllib.error
import urllib.request
from pathlib import Path

from _shared import REPO, fail, log, run

DOCS = REPO / "docs"


def install(_: argparse.Namespace) -> None:
    """Install the frozen documentation dependency set."""
    run(["bun", "install", "--frozen-lockfile"], cwd=DOCS)


def build(_: argparse.Namespace) -> None:
    """Build the static Fumadocs export."""
    run(["bun", "run", "build"], cwd=DOCS)


def verify(_: argparse.Namespace) -> None:
    """Build the docs from a frozen lockfile, as the PR gate requires.

    The Pages workflow deploys from `main` only, so without this a broken MDX
    file reaches `main` before anything compiles it. Rendering the export here
    catches it on the pull request instead.
    """
    run(["bun", "install", "--frozen-lockfile"], cwd=DOCS)
    run(["bun", "run", "build"], cwd=DOCS)


def _summary(note: str) -> None:
    target = os.environ.get("GITHUB_STEP_SUMMARY")
    if target:
        with Path(target).open("a", encoding="utf-8") as handle:
            handle.write(f"### GitHub Pages status\n\n{note}\n")


def _report(enabled: bool, note: str) -> None:
    target = os.environ.get("GITHUB_OUTPUT")
    if not target:
        fail("missing required environment variable: GITHUB_OUTPUT")
    with Path(target).open("a", encoding="utf-8") as handle:
        handle.write(f"enabled={'true' if enabled else 'false'}\n")
    print(note, flush=True)
    _summary(note)


def _pages_status() -> tuple[int, str]:
    token = os.environ.get("GH_TOKEN", "")
    api = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    request = urllib.request.Request(
        f"{api}/repos/{repository}/pages",
        headers={
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "Authorization": f"Bearer {token}",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status, response.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8", "replace")
    except OSError as error:
        log(f"pages status request failed: {error}")
        return 0, ""


def pages_status(_: argparse.Namespace) -> None:
    """Decide whether GitHub Pages can deploy this build."""
    if os.environ.get("ENABLE_PAGES", "") == "false":
        _report(False, "Skipped deployment: repository variable `ENABLE_PAGES` is `false`.")
        return
    if os.environ.get("EVENT_NAME") == "workflow_dispatch" and os.environ.get("DISPATCH_DEPLOY") == "false":
        _report(False, "Skipped deployment: manual `workflow_dispatch` requested build-only mode.")
        return
    code, body = _pages_status()
    if code == 200:
        build_type = json.loads(body or "{}").get("build_type", "")
        if build_type == "workflow":
            _report(True, "GitHub Pages is enabled (source: GitHub Actions). Deploying `docs/out`.")
        else:
            _report(
                False,
                "Skipped deployment: GitHub Pages `build_type` is "
                f"`{build_type or 'unknown'}` (expected `workflow`).",
            )
    elif code in (403, 404):
        _report(
            False,
            "Skipped deployment: GitHub Pages is not enabled or unavailable for this repository "
            f"(HTTP {code}).",
        )
    else:
        print(f"::warning::Unable to query GitHub Pages status (HTTP {code}); skipping deployment.")
        _report(False, f"Skipped deployment: unable to confirm GitHub Pages availability (HTTP {code}).")


def register(groups: argparse._SubParsersAction) -> None:
    """Attach the documentation commands to the root parser."""
    group = groups.add_parser("docs", help="Documentation build and Pages deployment steps.")
    commands = group.add_subparsers(dest="command", required=True, metavar="COMMAND")
    commands.add_parser("install", help="Install frozen documentation dependencies.").set_defaults(
        handler=install
    )
    commands.add_parser("build", help="Build the static Fumadocs export.").set_defaults(handler=build)
    commands.add_parser(
        "verify", help="Install frozen docs dependencies and build, as the PR gate does."
    ).set_defaults(handler=verify)
    commands.add_parser("pages-status", help="Decide whether Pages can deploy.").set_defaults(
        handler=pages_status
    )
