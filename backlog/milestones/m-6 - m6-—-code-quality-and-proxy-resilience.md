---
id: m-6
title: "M6 - Code quality and proxy resilience"
---

## Description

Refactor cross-cutting CLI and proxy code discovered by the clean-code audit. Establish narrow module responsibilities, extensible registry composition, bounded proxy resource use, and a selected strict Clippy baseline without adopting noisy lint groups wholesale.

## Exit criteria

- [ ] CLI configuration resolution, execution-session lifecycle, child-process environment construction, and package-manager configuration generation have distinct tested modules.
- [ ] A proxy execution session shuts down its listener and removes temporary configuration on every success and failure path through RAII ownership.
- [ ] Registry adapters are injected through a default factory or builder instead of being hard-coded in proxy binding.
- [ ] HTTP response state is represented without mutually inconsistent metadata/artifact boolean flags.
- [ ] Proxy request execution has an explicit bounded-concurrency policy and remains responsive during overload and shutdown.
- [ ] Selected Clippy lints are enforced after their existing findings are resolved; intentional filename-case policy and dependency-version exceptions are documented.

## Status

**Planned**.

## Tasks

- **ISO-19**: Decompose CLI execution sessions and environment injection.
- **ISO-20**: Make proxy adapter composition and response state extensible.
- **ISO-21**: Bound proxy request resources and harden portability edges.
- **ISO-22**: Establish selected strict Clippy and public API quality policy.
