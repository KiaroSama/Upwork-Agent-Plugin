# Native command-boundary fixtures

Reviewed and adapted from the owner's GPL-3.0-only SSE audit evidence (2026-10-15). These are persistent synthetic fixtures, not raw audit logs or private account data.

Linux CI uses `run.py` with `-I` to verify the packaged archive checksum, extract only its native executable into a fresh project-owned directory and run eight suites: CLI272 assertions, protocol30 cases, raw/framing20 cases, manual ordering8 cases and bridge ordering8 cases credential coordination3 cases (both writer orders and two bridges), gated initialization ping20 cases and bounded state-reader13 cases. No fixture contacts Upwork: a local CONNECT/TLS server terminates requests and rejects any other CONNECT target. Certificate overrides apply only to each child. Do not run Linux trust fixtures against a Windows executable.

The controller bounds each suite to180seconds and kills/reaps its owned process group on timeout. Individual native children have4-13second bounds. Fixture HTTP servers and reader threads are stopped/joined; no fixed sleep is used as readiness proof. One suite runs at a time. Runtime behavior tests remain in the four-platform Rust suite.

Requirements: Python3.10+ and cryptography50.0.2 (test-only CI virtual environment). Invocation from the repository root:

```bash
python3 -I tests/native/run.py --archive dist/upwork-agent-plugin-2.1.5-linux-x64.tar.gz --output .ci-work/native-acceptance
```

Output must be a new empty project-owned directory. UTF-8 JSON receipts and one UTC log per probe are retained; raw OAuth URLs/state/tokens and certificate private keys are never exported. Temporary fixture directories and the extracted executable are removed after success. A failed assertion is a real nonzero failure, not a retry or skip. Baseline audit receipts remain separate and are not counted as after-fix executions.
