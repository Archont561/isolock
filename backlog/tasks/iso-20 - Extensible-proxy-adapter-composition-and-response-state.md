---
id: ISO-20
title: Extensible proxy adapter composition and response state
status: Done
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
- [x] #1 `ProxyServer` accepts an injected adapter registry or builder and a public default factory supplies the current four adapters.
- [x] #2 Adding an adapter does not require editing loopback binding or HTTP server setup code.
- [x] #3 HTTP response classification uses an enum or equivalent closed representation; invalid simultaneous metadata/artifact state is unrepresentable.
- [x] #4 Compression, range transformation, and wire serialization are independently testable operations.
- [x] #5 Existing adapter and HTTP integration tests retain their registry response, Range, and Brotli behavior.
<!-- AC:END -->

## Notes

- Addresses OCP and encapsulation findings in `crates/isolock-proxy/src/server.rs`.
- Builds on the `RegistryAdapter` direction accepted in decision-4.

## Implementation evidence

Public `AdapterRegistry`, `default_adapters`, and `ProxyServer::bind_with_adapters` separate
composition from binding. Socket tests add an extension adapter without listener edits.
Closed metadata/artifact/text response state lives in `response.rs`; compression, borrowing
Range transformation, and wire writes are independently exercised. Existing four-registry,
Brotli, ETag, HEAD, and Range integration cases still pass.
