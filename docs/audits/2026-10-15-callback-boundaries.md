# OAuth callback boundary audit — 2026-10-15

## Mandatory review procedure

First read and obey the owner's actual private Rules, repository instructions and hooks. Execute the complete Spec Kit chain in the order required by those Rules, including current upstream/GitHub research and the required specification, plan, tasks, implementation, verification and integration records. Those private files are excluded from the public checkout; this audit does not claim to have executed unavailable hooks.

The auditing assistant may publish implemented fixes and regression tests in a PR but must not merge or close PRs/issues or update `main`. The owner's reviewing agent must review and resolve **all PRs under the Rules**: merge eligible, verified changes; otherwise correct and integrate the equivalent implementation before closing a superseded PR with a traceable receipt. Fix newly reproduced defects in the same reviewing session. A closed PR alone is not proof of a fixed defect. Reuse this correction rather than creating competing implementations or reopening previously verified findings without a new reproduction.

All published owner commits, historical and future, must have raw author and committer email `Kiaro.Sama.Dev@gmail.com`. Inspect `%ae` and `%ce`, not a mailmap or coauthor trailer. If a mismatch exists, retain a verified private recovery bundle, coordinate the affected refs under the Rules, preserve content/provenance, record an old-to-new mapping and publish only with an explicit expected-ref lease. Rerun final-head checks after history repair. Do not confuse GitHub's generated test-merge identities with owner branch history. The 43 commits reachable from the baseline `main` already conform; no rewrite of that history is required by this audit.

## Exact baseline and closure ledger

- Repository: `KiaroSama/Upwork-Agent-Plugin`.
- Baseline: `41c3035fe38ab1c0263014cbdc8fb95b97d13d81`, version `2.1.2`.
- Baseline tree: `3d17ef125a3e180d1bd9b05af4f490ca31e8e01d`.
- Previous [PR #7](https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/7) is merged. Its strict initialization schema/identity/version checks, pre-normalization mixed-envelope rejection, macOS prerequisites and canonical commit identities are retained.
- Earlier registration ownership/rollback, bounded imports, command validation, explicit public ports, route cancellation, locks, framing and heartbeat corrections are not reopened.
- The subsequent device-timezone metadata normalization remains scoped to existing device hints. This audit does not rewrite business dates, disable that feature, add proxy fallback, or migrate the MCP lifecycle.

Current baseline source was retrieved as a full Git bundle; pinned Rust `1.99.0` and public locked dependencies were exported through an isolated preparation workflow. Exported paths excluded checkout credentials and private state. Source/archive/compiler/cache checksums were checked before use. The helper was removed from the preparation branch ancestry; it is not part of the proposed application tree.

The baseline executable from [main CI 38624523313](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38624523313) was separately checksum/provenance checked. Executable SHA256: `35459cc9f3bb121027d4a574860a02b02a8faf99080b2c25261d97958a7bf0d4`.

## CB01 — stored default HTTP callback port is not the listener port

**Severity: medium, sign-in reliability.** Existing native/legacy client registration can legitimately name `http://127.0.0.1/oauth/callback` or explicitly `http://127.0.0.1:80/oauth/callback`. The baseline uses `Url::port().unwrap_or(0)` to choose the listener. The URL library omits a known default port even when explicitly supplied, so both forms bind an unrelated ephemeral port while the browser is directed to port 80. Sign-in cannot receive the intended callback.

**Native reproduction:** seed a disposable `.state/rust/client.json` with fixture client ID and either redirect URI. Serve synthetic OAuth metadata/registration/token and MCP responses through a local CONNECT/TLS fixture. Verify beforehand that port 80 is available to the fixture user. Launch the checksum-verified baseline with an isolated root and `sign-in`, read the displayed authorization URI in memory and send its callback to the advertised destination. Both stored forms reject the connection; no readiness or completion is produced. The harness terminates the waiting child after establishing the mismatch; it does not claim to have waited out the full 300-second application timeout. The unregistered ephemeral-port control succeeds.

**Cause:** default-port normalization was mistaken for an absent/unregistered callback. The same selection path could invent an ephemeral destination from malformed saved callback metadata or a stored port 0.

**Implemented fix:** private `src/auth/local_callback.rs` distinguishes absent registration from malformed stored callback data. It validates a saved local HTTP target before binding, uses `port_or_known_default()`, rejects port 0 and unsafe target components, and preserves the registered URI string when passing it to OAuth. A busy or unavailable saved port returns a fixed, credential-free error; it never silently picks another port. A new registration still binds IPv4 loopback port 0 and constructs its redirect using the actual assigned port. Existing `localhost` and `127.0.0.1` support, path/query data and private registration files are preserved.

Port 80 is not newly required for ordinary sign-in: only a registration already naming that port uses it. No administrator elevation, host firewall/trust change or deletion of saved clients/tokens is part of this repair.

## CB02 — byte-wise deadlines let a slow callback block valid callbacks

**Severity: medium, local callback availability.** The baseline applies a fresh five-second timeout to every byte and handles accepted sockets serially. A local connection sending bytes more frequently than every five seconds can monopolize the accept loop for most or all of the global sign-in window. A legitimate browser callback waits behind it.

**Native reproduction:** begin a synthetic sign-in, connect to its loopback callback first, send an initial byte and then another every 200 ms. Queue the genuine callback after the first connection is established. While the first socket continues to drip, the valid callback does not complete within 6.5 seconds on the baseline. After deliberately releasing the first socket, the genuine callback completes. This is an observed local availability defect, not a claim of remote account takeover or a bypass of state/PKCE/issuer validation.

**Implemented fix:** one five-second deadline covers the complete header read, not each byte. The helper still requires the entire CRLF-CRLF delimiter, consumes at most 8192 bytes and does not read a subsequent body. Incomplete EOF, overflow and timeout return fixed safe errors. The sign-in loop logs a content-free warning, drops the rejected socket and accepts the next callback. Browser acknowledgement is also best-effort and bounded to five seconds; a disconnected browser does not invalidate an already successful token exchange. The existing 300-second overall sign-in limit, route cancellation and OAuth/MCP timeouts remain.

The corrected native control completes the legitimate callback while the first client is still attempting to drip. This does not claim unrestricted resistance to arbitrarily many hostile local clients; no unbounded concurrent callback service was introduced.

## Implementation and regression coverage

Changes are limited to `src/auth.rs`, the new private callback helper, one dev-only Tokio feature declaration and this audit. Runtime dependency versions and `Cargo.lock` are unchanged. No release assets, release version, proxy route, credentials, registration ownership, global settings or marketplace operation is modified.

Fifteen new Rust tests cover:

1. Implicit/explicit default ports for both supported hosts; nondefault/boundary ports; query preservation; invalid schemes/hosts/ports and safe errors.
2. Absent versus malformed stored callback data; actual ephemeral loopback binding; occupied fixed-port refusal; preservation of the original registered URI spelling.
3. Complete headers, delimiter split across reads, no body consumption, exact 8192-byte acceptance, bounded overflow and incomplete/empty EOF.
4. Silent and dripped connections using Tokio's paused clock, so repeated bytes provably cannot reset the deadline; stalled/disconnected acknowledgement writes.

`tokio/test-util` is enabled only through `[dev-dependencies]` for deterministic virtual-time tests. No sleep-based retry-to-green or production timeout bypass is added. The existing four-platform CI automatically discovers these library tests, retaining its zero-retry nextest profile, strict Clippy, package/launcher/cleanup checks and security gates. Dependabot already covers Cargo and GitHub Actions daily; another updater or forced dependency upgrade is not justified.

## Evidence and final acceptance

Fresh local baseline compilation/test execution: **98 passed, 0 failed, 0 ignored**. Fresh baseline native checks: **272 CLI assertions in 119 bounded child processes**, and the existing **30-case initialization/readiness matrix**, all passed. These results keep previous fixes closed while separately exposing CB01/CB02.

Fresh corrected local execution: formatting, `clippy --offline --locked --all-targets -- -D warnings`, and all-target tests passed: **113 tests, 0 failures, 0 ignored**. The actual compiled binary passed all four native callback cases: new registration, both saved default-port representations and recovery while a first socket is still dripping. Registration data remained unchanged. Broader native CLI/protocol results and final hosted receipts are recorded separately in the PR and downloadable handoff, tied to the exact head; do not substitute a prior-head green run.

Before integration, require all final four-platform tests/builds/launcher/cleanup/packages, dependency audit, CodeQL and Dependency Review to conclude successfully. Download the final Linux artifact, verify outer and inner checksums and `release.json`, then rerun callback, CLI and protocol controls. When GitHub builds a synthetic PR merge, compare its Git tree with the proposed head before attributing its artifact to that content. A queued/cancelled job or an uploaded artifact alone is not final CI success.

The harnesses use disposable loopback-only synthetic OAuth/TLS services, separate ephemeral certificates and per-child trust variables. They never forward requests to Upwork or export tokens, authorization URLs/states, headers, certificate private keys or real account data. Baseline default-port failure children are deliberately reaped; unperformed waits/tests are not counted. Real Upwork/browser/Claude account integration and private hooks remain separate owner-side checks, not falsely certified here.

After review, perform the remaining commit/push/integration actions under the Rules, verify the integrated default branch and any newly published artifacts, and issue an evidence-backed DONE receipt. Do not leave a reproduced defect or an unexplained PR disposition for another round. If a real external permission/account prerequisite prevents a gate, identify it precisely rather than manufacturing a passing result.

## Optional improvements, not additional confirmed defects

Evaluate a bounded multi-connection callback design only if actual usage requires it; retain bounded memory/concurrency and exact-once exchange semantics rather than adding a generic web server unnecessarily. A curated malformed-callback/property-test corpus may expand parser coverage, but is not needed to replace the deterministic regressions above. Evaluate release artifact attestations separately under the owner's Rules. None of these suggestions requires reopening a verified defect or delaying this narrow correction for a redesign.

## Primary references

- [URL 2.5.8: `port` and `port_or_known_default`](https://docs.rs/url/2.5.8/url/struct.Url.html#method.port_or_known_default).
- [Tokio timeout semantics](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) and [paused time / test-util](https://docs.rs/tokio/latest/tokio/time/fn.pause.html).
- [RFC 8252: native OAuth loopback redirect and listener](https://www.rfc-editor.org/rfc/rfc8252#section-7.3).
- [MCP initialization and readiness ordering](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle).
- [Pinned RMCP SDK source](https://github.com/modelcontextprotocol/rust-sdk/tree/rmcp-v3.5.1).
- [Baseline CI](https://github.com/KiaroSama/Upwork-Agent-Plugin/blob/41c3035fe38ab1c0263014cbdc8fb95b97d13d81/.github/workflows/ci.yml) and [Dependabot configuration](https://github.com/KiaroSama/Upwork-Agent-Plugin/blob/41c3035fe38ab1c0263014cbdc8fb95b97d13d81/.github/dependabot.yml).
- [Git commit-tree identities](https://git-scm.com/docs/git-commit-tree), [recovery bundles](https://git-scm.com/docs/git-bundle) and [explicit push leases](https://git-scm.com/docs/git-push#Documentation/git-push.txt---force-with-lease).
