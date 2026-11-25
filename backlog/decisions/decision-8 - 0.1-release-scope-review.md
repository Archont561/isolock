---
id: decision-8
title: 0.1 release scope review and support claim boundary
date: '2026-11-25 16:00'
status: proposed
---

## Context

ISO-25 requires an explicit release-scope review before platform CI or release claims are
broadened. The 0.1 support contract lives in `docs/content/docs/support.mdx`, and its claims
are backed by named tests in required CI jobs.

A review is not a re-reading of that page. It is the point where a person decides the claim
set is the right one, and accepts the consequences of the claims being wrong. Three things
make that decision necessary rather than automatic.

**The claims were recently altered.** ISO-27 corrected a stale statement: the contract said
package-manager transactions had never been exercised end to end, which stopped being true
in ISO-36. The corrected text claims less than the evidence could support, and it names
pip and npm as unexercised.

**Evidence is job status only.** CI uploads no per-claim report artifact. A reviewer
approving a claim is trusting a green job, not inspecting a stored result.

**Some claims rest on a narrower base than their wording suggests.** `cargo add` resolves
from the vendored directory source rather than over the loopback proxy, so the "real
client" claim is true of the client but not of the proxy path for Cargo.

## Decision

The 0.1 claim set is approved as written, subject to the exclusions recorded below. This
decision approves the **scope**, not the artifact: it does not tag a release, and ISO-29
remains the go/no-go gate that consumes it.

### Approved claims

| Claim | Basis | Reviewer check |
| --- | --- | --- |
| Standalone CLI archive on `linux-x86_64`, `macos-arm64`, `windows-x86_64` | Native release-artifact smoke per platform | Job status per platform |
| Conda package on `linux-64`, `osx-arm64`, `win-64` | `rattler-build` recipe test, including `plan --ci` | Job status in the release run |
| Locked-profile materialization on `linux-64` (`rust-pixi`, `rust-standalone`) | `tools test profile-materialize`, one job per profile | Job status per profile |
| Reviewed storage publication | `ingestion_profiles`, `reviewed_ingestion` | Job status |
| Registry proxy protocols for PyPI, Conda, npm, Cargo | Four-protocol `cli_test` journey; artifact replay after upstream deletion; unreviewed request stays 403 | Job status |
| Real clients: `pixi add`, `cargo add` | Three release-gated journeys in the profile matrix | Job status per profile |

### Accepted exclusions

- No registry version solving, and no transitive dependency discovery for any ecosystem.
- pip and npm are not exercised as package-manager clients.
- `cargo add` resolves from the vendored directory source, not through the proxy.
- Offline profile materialization is not claimed for macOS or Windows.
- Only the three named release targets are supported. Linux `aarch64`, Intel macOS, and
  Windows GNU builds are explicitly unsupported; cross-compilation success is not evidence.
- Connected proxy-miss acquisition, automatic legacy/sharded storage migration, and
  old-epoch deletion are outside the release contract.
- Git transport may transfer a broader snapshot before retaining the selected closure.

## Consequences

- **Positive**: The claim set is narrower than the architecture, so a supported path is
  expected to work rather than hoped to work.
- **Positive**: Cross-compilation and successful builds are recorded as non-evidence, which
  prevents the matrix from drifting open without a decision.
- **Positive**: The `cargo add` and pip/npm limits are stated, so a reader cannot infer
  broader client support than the tests establish.
- **Negative**: Supporting Linux `aarch64` or Intel macOS later reopens this review, because
  those targets require native runners and a new acceptance claim.
- **Negative**: Because evidence is job status rather than a stored artifact, re-verifying a
  claim later depends on the run being retained.
- **Risk accepted**: A claim whose test becomes vacuous would stay green. The mitigation is
  not in this decision; it is the per-claim sabotage check performed when each was added,
  recorded in the task notes for ISO-25, ISO-26, and ISO-27.

## Approval

AC #4 is satisfied only by a maintainer signature. Filling this section is the act that
closes ISO-25.

| Field | Value |
| --- | --- |
| Reviewer | *(pending)* |
| Role | *(pending)* |
| Date | *(pending)* |
| Candidate commit reviewed | *(pending)* |
| CI run on that commit | *(pending)* |
| Non-publishing release rehearsal run | *(pending)* |
| Decision | *(pending: GO / NO-GO / HOLD)* |

Until every field above is filled by a person, this decision is `proposed`, ISO-25 AC #4
stays open, and ISO-29 must not record a GO.

## Verification performed while drafting

Recorded so a reviewer does not have to take the table on trust, and so an unfilled signature
is visible as the only outstanding item.

- Every test name cited in the support table resolves to a real function under
  `crates/isolock/tests/`.
- Every runner cited exists as a job in `.github/workflows/ci.yml` or `release.yml`.
- The Conda packages are built on the native matrix runner that also builds the standalone
  binary, matching the "native matrix runners" wording.
- `release host-smoke` genuinely refuses a foreign target, so the cross-compilation
  exclusion is enforced rather than merely written down.

This section records what a machine can check. It is not a substitute for the signature.
