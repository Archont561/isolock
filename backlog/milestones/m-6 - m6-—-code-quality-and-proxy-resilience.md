---
id: m-6
title: "M6 - Code quality and proxy resilience"
---

## Description

Refactor cross-cutting CLI and proxy code discovered by the clean-code audit. Establish narrow module responsibilities, extensible registry composition, bounded proxy resource use, and a selected strict Clippy baseline without adopting noisy lint groups wholesale.

## Exit criteria

- [x] CLI configuration resolution, execution-session lifecycle, child-process environment construction, and package-manager configuration generation have distinct tested modules.
- [x] A proxy execution session shuts down its listener and removes temporary configuration on every success and failure path through RAII ownership.
- [x] Registry adapters are injected through a default factory or builder instead of being hard-coded in proxy binding.
- [x] HTTP response state is represented without mutually inconsistent metadata/artifact boolean flags.
- [x] Proxy request execution has an explicit bounded-concurrency policy and remains responsive during overload and shutdown.
- [x] Selected Clippy lints are enforced after their existing findings are resolved; intentional filename-case policy and dependency-version exceptions are documented.

## Status

**Implemented** (ISO-19–22).

Resource/session/configuration contracts have executable `rstest-bdd` bindings. Local
validation passes the offline workspace and Nextest suites (120 tests; two opt-in real-tool
tests are run separately), strict all-target Clippy, formatting, cargo-deny bans/licenses/sources,
51 repository rules, workflow/shell checks, and touched Markdown linting. Both real Pixi
cold-cache linkage and standalone Rust offline compilation also pass. Regenerated LCOV
reports 84.28% line coverage. The resource suite additionally passes five repeated runs.

See [proxy execution and limits](../../docs/content/docs/proxy.mdx) for overload/shutdown
semantics and trusted-adapter limitations, and [Rust quality policy](../../docs/content/docs/quality.mdx)
for selected lints, error documentation, archive case, and dependency-audit exclusions.

## Tasks

- **ISO-19**: Decompose CLI execution sessions and environment injection.
- **ISO-20**: Make proxy adapter composition and response state extensible.
- **ISO-21**: Bound proxy request resources and harden portability edges.
- **ISO-22**: Establish selected strict Clippy and public API quality policy.
