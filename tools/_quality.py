"""Hermetic quality, test, coverage, and profile commands."""

from __future__ import annotations

import argparse
import os

from _shared import REPO, merged_env, run

PROFILE_JOURNEYS = {
    "rust-pixi": [
        "real_pixi_restores_the_selected_profile_from_a_cold_offline_cache",
        "real_pixi_conda_profile_roundtrips_through_git_storage_and_installs_offline",
        "real_pixi_add_resolves_from_the_generated_channel",
        "real_pixi_add_reports_a_miss_for_an_unreviewed_package",
    ],
    "rust-standalone": [
        "real_standalone_rust_builds_a_locked_registry_dependency_offline",
        "real_standalone_rust_builds_a_locked_git_dependency_offline",
        "real_cargo_add_adds_a_reviewed_crate_offline",
    ],
}


def fmt(_: argparse.Namespace) -> None:
    """Check Rust formatting without rewriting sources."""
    run(["cargo", "fmt", "--check"])


def clippy(_: argparse.Namespace) -> None:
    """Run the blocking Clippy policy over every workspace target."""
    run(["cargo", "clippy", "--workspace", "--all-targets", "--offline", "--", "-D", "warnings"])


def deny(_: argparse.Namespace) -> None:
    """Audit bans, licenses, and sources against the restored registry."""
    run(["cargo", "deny", "--offline", "check", "bans", "licenses", "sources"])


def lint(_: argparse.Namespace) -> None:
    """Enforce the repository, schema, and workflow policy rules."""
    run(["alint"])


def quality(_: argparse.Namespace) -> None:
    """Run the complete blocking quality gate."""
    for command in (fmt, clippy, deny, lint):
        command(argparse.Namespace())


def nextest(_: argparse.Namespace) -> None:
    """Run the fast hermetic nextest suite."""
    run(["cargo", "nextest", "run", "--workspace", "--offline"])


def test(_: argparse.Namespace) -> None:
    """Run the BDD, property, and integration suites."""
    run(["cargo", "test", "--workspace", "--offline"])


def coverage(_: argparse.Namespace) -> None:
    """Generate the full-workspace LCOV report under the coverage gate."""
    run(["sh", "scripts/coverage.sh"])


def acceptance(_: argparse.Namespace) -> None:
    """Exercise the real CLI, epoch rollover, and ingestion profile journeys."""
    run(
        [
            "cargo",
            "nextest",
            "run",
            "--package",
            "isolock",
            "--test",
            "cli_test",
            "--test",
            "epoch_rollover",
            "--test",
            "ingestion_profiles",
            "--offline",
        ]
    )


def all_suites(_: argparse.Namespace) -> None:
    """Run workspace, BDD, and acceptance suites as one release gate."""
    for command in (nextest, test, acceptance):
        command(argparse.Namespace())


def _run_profile_tests(profile: str, names: list[str]) -> None:
    env = merged_env(
        {
            "ISOLOCK_TEST_PROFILE": profile,
            "ISOLOCK_TEST_PLATFORM": os.environ.get("ISOLOCK_TEST_PLATFORM", "linux-64"),
            "ISOLOCK_TEST_PIXI": str(REPO / ".pixi/tools/linux-64/pixi"),
            "ISOLOCK_TEST_RUST_PREFIX": str(REPO / ".pixi/envs/default"),
        }
    )
    for name in names:
        run(
            [
                "cargo",
                "test",
                "--package",
                "isolock",
                "--test",
                "real_profile",
                "--offline",
                name,
                "--",
                "--ignored",
                "--nocapture",
            ],
            env=env,
        )


def profile_materialize(_: argparse.Namespace) -> None:
    """Materialize only the profile selected by the CI matrix."""
    selected = os.environ.get("ISOLOCK_TEST_PROFILE")
    if selected not in PROFILE_JOURNEYS:
        raise SystemExit(f"unsupported ISOLOCK_TEST_PROFILE: {selected!r}")
    _run_profile_tests(selected, PROFILE_JOURNEYS[selected])


def profile_journeys(args: argparse.Namespace) -> None:
    """Materialize every release-gated locked profile journey."""
    profiles = args.profiles or list(PROFILE_JOURNEYS)
    for profile in profiles:
        if profile not in PROFILE_JOURNEYS:
            raise SystemExit(f"unsupported profile: {profile!r}")
        _run_profile_tests(profile, PROFILE_JOURNEYS[profile])


def build_cli(_: argparse.Namespace) -> None:
    """Build the CLI from the reviewed lockfile without registry access."""
    run(["cargo", "build", "--locked", "--offline", "--package", "isolock"])


def register(groups: argparse._SubParsersAction) -> None:
    """Attach the quality and test commands to the root parser."""
    group = groups.add_parser("quality", help="Formatting, lint, and supply-chain gates.")
    commands = group.add_subparsers(dest="command", required=True, metavar="COMMAND")
    for name, handler, help_text in (
        ("fmt", fmt, "Check Rust formatting."),
        ("clippy", clippy, "Run blocking Clippy lints."),
        ("deny", deny, "Audit bans, licenses, and sources."),
        ("lint", lint, "Enforce repository and workflow policy."),
        ("all", quality, "Run every blocking quality gate."),
    ):
        commands.add_parser(name, help=help_text).set_defaults(handler=handler)

    tests = groups.add_parser("test", help="Hermetic test, coverage, and profile suites.")
    suites = tests.add_subparsers(dest="command", required=True, metavar="COMMAND")
    for name, handler, help_text in (
        ("nextest", nextest, "Run the nextest workspace suite."),
        ("bdd", test, "Run BDD, property, and integration tests."),
        ("coverage", coverage, "Generate the gated LCOV report."),
        ("acceptance", acceptance, "Run CLI, epoch, and ingestion acceptance."),
        ("all", all_suites, "Run every release test suite."),
        ("build", build_cli, "Build the CLI offline from the lockfile."),
    ):
        suites.add_parser(name, help=help_text).set_defaults(handler=handler)

    materialize = suites.add_parser(
        "profile-materialize",
        help="Materialize the ISOLOCK_TEST_PROFILE matrix selection.",
    )
    materialize.set_defaults(handler=profile_materialize)

    journeys = suites.add_parser(
        "profile-journeys",
        help="Materialize the locked profiles required by release acceptance.",
    )
    journeys.add_argument(
        "--profile",
        dest="profiles",
        action="append",
        choices=sorted(PROFILE_JOURNEYS),
        help="Restrict the run to one profile; repeat to select several.",
    )
    journeys.set_defaults(handler=profile_journeys)
