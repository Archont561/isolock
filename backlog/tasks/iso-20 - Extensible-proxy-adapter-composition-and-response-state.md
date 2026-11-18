---
id: ISO-20
title: Extensible proxy adapter composition and response state
status: To Do
assignee: []
created_date: '2026-11-18 11:00'
labels: []
milestone: m-6
dependencies: [ISO-5]
type: task
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Separate default registry-adapter construction from proxy binding and replace independently mutable HTTP response classification booleans with a closed response-kind model. Preserve the current Conda, PyPI, npm, and Cargo routing behavior while making a new adapter additive rather than a server edit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 `ProxyServer` accepts an injected adapter registry or builder and a public default factory supplies the current four adapters.
- [ ] #2 Adding an adapter does not require editing loopback binding or HTTP server setup code.
- [ ] #3 HTTP response classification uses an enum or equivalent closed representation; invalid simultaneous metadata/artifact state is unrepresentable.
- [ ] #4 Compression, range transformation, and wire serialization are independently testable operations.
- [ ] #5 Existing adapter and HTTP integration tests retain their registry response, Range, and Brotli behavior.
<!-- AC:END -->

## Notes

- Addresses OCP and encapsulation findings in `crates/isolock-proxy/src/server.rs`.
- Builds on the `RegistryAdapter` direction accepted in decision-4.
