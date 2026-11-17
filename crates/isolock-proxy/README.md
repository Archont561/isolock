# isolock-proxy

Hermetic HTTP/1.1 loopback proxy for Conda, PyPI, NPM, and Cargo. It binds only to a loopback IP; port `0` asks the operating system for a collision-free port.

## Serving

```rust,no_run
use isolock_cas::InMemoryCas;
use isolock_core::AirlockMode;
use isolock_plan::AirlockPlan;
use isolock_proxy::{ProxyConfig, ProxyServer};

let server = ProxyServer::bind(
    ProxyConfig::new("127.0.0.1", 0, AirlockMode::LocalAirlock),
    AirlockPlan::new(),
    InMemoryCas::new(),
)?;
println!("proxy origin: {}", server.base_url());
server.serve()?;
# Ok::<(), std::io::Error>(())
```

`ProxyServer::spawn()` provides a stoppable background `ProxyHandle`. Requests support `GET`, `HEAD`, and single byte ranges. Textual registry metadata is Brotli-compressed only when the client advertises `br`; package bytes are returned unchanged. `Ci` and `LocalAirlock` return `403` for registry cache misses, and the server never makes upstream requests in any mode.

The CLI's `isolock serve` and `isolock exec <command> [args...]` load a JSON `AirlockPlan` from `ISOLOCK_PLAN` (default `.isolock/plan.json`) and each planned CAS blob from `ISOLOCK_CAS_DIR` (default `.isolock/cas/sha256/<xx>/<yy>/<digest>`). `exec` starts the listener on a dynamic port, injects local pip/uv, npm, Cargo, and Conda/Pixi settings, then shuts the listener down and propagates the child's exit code. Conventional HTTP, HTTPS, and all-proxy environment variables point to the same listener, while loopback is excluded via `NO_PROXY`; the listener never forwards upstream or accepts `CONNECT`. Cargo gets an isolated registry-configured home at `.isolock/cargo-home` (override with `ISOLOCK_CARGO_HOME`). Pixi gets a temporary mirror config that redirects common Conda channel hosts to the loopback listener; an existing explicit `PIXI_CONFIG_FILE` is merged, with those common-host mirror rules replaced by local routes. Project-local `.pixi/config.toml` remains in effect.

## Registry routes

- PyPI PEP 691: `/pypi/simple/<project>/` and `/pypi/packages/<wheel>`.
- Conda: `/conda/channel/<subdir>/repodata.json` (also `current_repodata.json`) and mirrored channel paths `/conda/channel/<channel>/<subdir>/...`.
- NPM: `/npm/<package>` (including URL-encoded scopes) and `/npm/<package>/-/<tarball>`.
- Cargo sparse registry: `/cargo/config.json`, RFC 2789 sharded index paths below `/cargo/`, and `/cargo/api/v1/crates/<crate>/<version>/download`.

## Plan metadata

`PlannedArtifact` retains its compact name/version/registry/SHA-256 identity. Use `AirlockPlan::add_with_metadata` to retain the original filename and the registry metadata captured by ingest. These fields are needed because filenames, Python compatibility tags, package dependencies, and Conda build/subdir data cannot be reconstructed from a digest alone.

`PlannedArtifactMetadata` is serialized inline in the JSON airlock plan. `registry_metadata` may contain:

- PyPI: `{"name":"requests","version":"2.32.3","requires-python":">=3.8"}` (or equivalent fields directly on the metadata object).
- Conda: the archive's `info/index.json` object, or `{"package_record": <rattler PackageRecord JSON>}`; also set `subdir` and `filename`.
- NPM: `{"package_json": <package/package.json object>}`.
- Cargo: `{"Cargo.toml":"<normalized crate manifest>"}` or `{"cargo_index": <RFC 2789 version object>}`.

The original archive bytes remain in CAS and are served raw. Index synthesis uses `rattler_conda_types` and `tame-index` for the Conda and Cargo formats respectively; PyPI and NPM metadata are rendered as JSON.
