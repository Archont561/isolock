---
id: doc-9
title: Typed configuration and plan locking
type: architecture
created_date: '2026-11-18 10:00'
updated_date: '2026-11-18 10:00'
---

## Typed Configuration and Plan Locking

### Configuration Scope

`isolock.toml` is the project-owned declaration of profile intent. It configures profile names, platforms, lockfile inputs, runtime-provider policy, materializers, storage references, and operational defaults. It does not replace ecosystem lockfiles; Pixi, Cargo, PyPI, and JavaScript lockfiles remain the source of artifact resolution.

Figment loads the typed CLI configuration from layered TOML and environment providers. The CLI is the only crate that performs this loading. Core planning receives a resolved request, and POSIX bootstrap consumes generated manifests rather than parsing project configuration.

### Precedence and Provenance

Configuration resolves in this order:

```text
compiled defaults
  < user operational configuration
  < project isolock.toml
  < generated project-local configuration
  < ISOLOCK_* variables
  < explicit CLI flags
```

The command `isolock config explain <path>` reports both the effective value and the provider that supplied it. `isolock config show --profile <name>` renders the resolved typed configuration with secrets redacted.

### Locked Inputs

A profile plan records a canonical digest of every value that can alter its artifact closure or trust boundary:

- selected profile and platform;
- selected Pixi environment and Pixi lock digest;
- Cargo, PyPI, and JavaScript lockfile digests;
- seed Isolock and Pixi provider kind, version, and digest;
- storage ref resolved to a commit;
- registry/channel metadata digests;
- materializer and vendor derivation policy.

CI uses locked mode. It rejects an environment, user configuration, or CLI override that would change any recorded input before it downloads or materializes an artifact.

### Operational Overrides

Operational values do not change closure identity and may be overridden locally: CAS/cache locations, link strategy, download concurrency, temporary directories, or selected local proxy port. They are intentionally excluded from the plan digest unless a particular materializer treats them as semantic.

Secrets are never serialized into plans, manifests, or `config show`. Registry credentials are obtained from standard package-manager keychains, explicit external secret references, or CI secret injection.

### Schema Evolution

The project configuration carries an integer schema version. Isolock rejects unsupported future schemas and provides migration diagnostics for retired keys. Unknown keys are rejected by default to prevent typographical changes from silently weakening an airlock policy.
