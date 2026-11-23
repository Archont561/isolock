"""Repository tool CLI.

Workflow YAML stays declarative: every `run:` step is one line that calls
`pixi run tools <command>`. The commands themselves live here.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import _airlock  # noqa: E402 - path bootstrap above
import _docs  # noqa: E402 - path bootstrap above
import _quality  # noqa: E402 - path bootstrap above
import _release  # noqa: E402 - path bootstrap above
import _sandbox  # noqa: E402 - path bootstrap above


def build_parser() -> argparse.ArgumentParser:
    """Assemble the command tree from each tool module."""
    parser = argparse.ArgumentParser(
        prog="tools",
        description="Repository verification, release, and automation commands.",
    )
    groups = parser.add_subparsers(dest="group", required=True, metavar="GROUP")
    for module in (_quality, _release, _sandbox, _airlock, _docs):
        module.register(groups)
    return parser


def main(argv: list[str] | None = None) -> int:
    """Dispatch a tool command and return its exit status."""
    args = build_parser().parse_args(argv)
    args.handler(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
