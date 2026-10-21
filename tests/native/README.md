# Native command-boundary fixtures

Reviewed and adapted from the owner's GPL-3.0-only SSE audit evidence (2026-10-15). These are persistent synthetic fixtures, not raw audit logs or private account data.

Linux CI uses the Rust `native-acceptance` Cargo example to verify the packaged archive checksum, extract only its native executable into a fresh project-owned directory and run the eleven original suites plus causal-oracle controls and five ordinary POST/listening-GET cases: CLI272 assertions, protocol30 cases, raw/framing20 cases, manual ordering8 cases and bridge ordering8 cases, credential coordination3 cases (both writer orders and two bridges), gated initialization ping20 cases and bounded state-reader21 cases including four valid dispatch controls, plus one 8,192-profile import/reimport scaling case, two observed-stop route-revision cases and28HTTP media/body boundary cases. No fixture contacts Upwork: a local CONNECT/TLS server terminates requests and rejects any other CONNECT target. Certificate overrides apply only to each child. Do not run Linux trust fixtures against a Windows executable.

The controller bounds each suite to180seconds and kills/reaps its owned process group on timeout. Individual native children have4-13second bounds. Fixture HTTP servers and asynchronous reader tasks are stopped/joined; no fixed sleep is used as readiness proof. One suite runs at a time. Runtime behavior tests remain in the four-platform Rust suite.

Requirements: the pinned Rust toolchain and existing Cargo dev dependencies, plus Ubuntu's OpenSSL, tar, sha256sum, mkfifo, kill and `/usr/bin/time` tools. No Python, pip or cryptography installation. The runner is test tooling only and is never included in production archives. Invocation from the repository root in GitHub-hosted Linux CI:

```bash
cargo run --locked --example native-acceptance -- --archive dist/upwork-agent-plugin-2.1.13-linux-x64.tar.gz --output .ci-work/native-acceptance
```

Output must be a new empty project-owned directory. UTF-8 JSON receipts and one UTC log per probe are retained; raw OAuth URLs/state/tokens and certificate private keys are never exported. Temporary fixture directories and the extracted executable are removed after success. A failed assertion is a real nonzero failure, not a retry or skip. Baseline audit receipts remain separate and are not counted as after-fix executions.

The route-revision fixture pauses only its owned Linux bridge with an observed SIGSTOP barrier, changes the route through real CLI commands and verifies zero queued outbound pings after a completed cycle. An inactive-profile edit is the live positive control. The separate three-case build-policy harness runs on all four platforms and does not substitute for real compilation.
