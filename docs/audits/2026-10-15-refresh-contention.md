Before any work, read and obey the owner's actual private Rules, repository instructions and hooks. Execute the complete Spec Kit chain prescribed in those Rules; do not invent substitute Rules or claim unpublished hooks ran. This document is the English implementation and review handoff for the requested audit.

# OAuth refresh contention audit and integration handoff

Repository: [KiaroSama/Upwork-Agent-Plugin](https://github.com/KiaroSama/Upwork-Agent-Plugin)  
Audit date: 2026-10-15  
Implemented correction: [PR #12](https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/12)  
Baseline: `810844fc68ed23d80e8969b1a80d9d65011d3bec`, version `2.1.3`  
Baseline tree: `f3dc29565d6429b4eecafb3b8852530eb2460f7f`  
First verified repair: `81e9d601bc7e7549572e57865501c9473254a201`  
First repair tree: `98844e10a22c7e186c1f84ec7c8e1baf6567f36d`

The auditing assistant opens/updates PRs only: it must not merge or close a PR or issue, publish a release, alter installed state, or update main. The owner's reviewing agent has the integration responsibility described below. Do not confuse a synthetic GitHub PR merge ref with an actual main integration.

## Executive result and non-repetition boundary

One new reproducible availability defect, RC01, was found in the credential-store refresh coordination. The implementation and regressions are already in PR #12, not left as an instruction-only proposal. It is independent of the previously merged SSE terminal-response correction in PR #11. Previously fixed registration, CLI/import bounds, explicit ports, raw initialization envelopes, callback deadlines and route cancellation remain protected by the existing tests; they are not reclassified as new findings.

The local execution service became unavailable during this session before Rust compilation. The test executions below are actual GitHub-hosted runner executions. No local compilation, live Upwork login, paid operation, proposal submission, message sending or real-account mutation is claimed. Tests use disposable roots and synthetic credentials, not user state. Private Rules/hooks are intentionally excluded from the published checkout, so the local reviewing agent must execute them before integration.

## RC01 — concurrent OAuth refresh fails immediately on a healthy busy lock

Priority: P2 correctness/availability. Scope: multiple OAuth managers or processes sharing the same installation credential store and overlapping token refresh.

### Root cause

At the baseline, `src/credentials.rs::Store::acquire_refresh_guard` calls `storage::Lock::acquire` for `.state/rust/refresh.json`. That generic lock acquisition is deliberately fail-fast. A healthy competing owner therefore produces an error, which the credential store maps to a generic `AuthError::CredentialStoreError`.

The pinned rmcp 3.5.1 `get_access_token` path invokes `refresh_token` for expired/nearly expired credentials. `refresh_token` acquires the credential-store guard before reloading credentials and making its token exchange. An immediately rejected guard means the second manager returns an authentication/storage failure even when the first manager is performing a valid refresh and will shortly release ownership. The observed defect is the second token-acquisition failure; no real production disconnection or account compromise is asserted.

### Deterministic reproduction and counterfactual

The same two desired-contract integration tests were compiled against unchanged baseline production code, then against the repaired code in one isolated runner checkout:

1. Hold a guard from one `Store`, then poll acquisition from an independently constructed `Store` using the same root. On the baseline the second future completes with an error, rather than remaining pending while the owner holds the lock.
2. Seed expired synthetic credentials. Construct two real `AuthorizationManager` instances with that same store and a loopback HTTP token endpoint. Gate the first exchange after the server observes it. Start the second manager's `get_access_token` while the first is held. On the baseline it returns `CredentialStoreError` before the gate is released.

[Verification run 38752019023](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38752019023) records successful compilation and exactly two expected baseline assertion failures: `0 passed; 2 failed`, with the explicit contention messages. A compile error was not accepted as a reproduction. After applying the fix, both tests passed, strict all-target Clippy passed, and all 137 Linux tests then present passed with zero failed or ignored tests. The expected red test command printed a generic ERROR trap line before the workflow checked its expected result; the overall verification job succeeded only after both red assertions and the green suite were validated. No assertion retry-until-green was used.

[Before/after evidence artifact](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38752019023/artifacts/11981148958) contains `before.log`, `after.log`, `clippy.log`, `repair.patch`, the verified source archive and commit receipt. Artifact SHA256: `0bff2904694102f351f1d0397fd9b57d91ac065529a9ccbe3528f1ad608420b1`.

### Implemented repair

`src/credentials/refresh_lock.rs` is a private refresh-specific helper. It uses the existing `Lock::try_acquire`, waits asynchronously for 50 milliseconds only on real contention, and applies a single 60-second total waiting deadline. The uncontested path completes immediately. Filesystem errors, non-file lock destinations and linked lock destinations remain immediate sanitized failures rather than entering the contention loop.

`Store::acquire_refresh_guard` delegates only its refresh ownership to this helper and returns the same SDK guard type. The underlying OS lock, lock-file identity and RAII release remain unchanged. No lock file is deleted to steal ownership. Ordinary configuration writes continue using the original fail-fast lock policy.

Cancellation drops the polling future. No blocking worker is spawned, so cancellation cannot leave a worker that acquires a lock later and retains it unexpectedly. Production route guards can cancel the wait when Block or another observed route change invalidates the operation. The existing outer timeout/cancellation behavior still applies. As with other synchronous filesystem operations, a stalled operating-system filesystem call is not made preemptible by an async timeout; this is not a claim of hard real-time filesystem guarantees.

### Token rotation and replay boundaries

Do not describe this as single-flight deduplication. The pinned SDK reloads the store after acquiring the guard, but can still perform a second serialized refresh. The integration fixture requires the second exchange to use the newly rotated refresh token; it rejects stale or extra exchanges. Both managers succeed, there are exactly two serialized exchanges using successive tokens, and a later `get_access_token` uses the stored unexpired token without a third exchange.

The new helper polls only OS-lock acquisition. It sends no HTTP request, retries no token exchange and replays no business operation. Do not silently redesign SDK refresh semantics, automatically retry proposals/messages, or introduce a direct-network fallback to address RC01.

## Regression coverage and evidence interpretation

The first repair adds six private helper tests (one Unix-specific) and two integration tests. A same-session test-only extension adds five test functions, including an explicitly invoked child helper, for cross-process release, recovery after terminating the owning child, cancellation by the production `Route::guard`, and independence of separate installation roots. The child helper is inert without its fixture environment and uses only a canonical `.ci-work` root. Owned child processes are reaped; every readiness/wait loop has a deadline.

The private tests use virtual time for the total deadline, delayed release and cancellation instead of waiting 60 real seconds. Integration tests exercise actual filesystem ownership and a real loopback HTTP token endpoint. These are complementary checks, not proof of all possible schedules. A counted Rust test function may contain multiple cases, and the child helper is not a separate user-facing feature test.

Final-head results must be read from PR #12 and its linked run receipts. The first-repair 137-test result is not automatically attributed to a later head. The final acceptance set includes all existing tests plus the extension, four-platform nextest, strict Clippy, source inventory, native build/launcher/cleanup/package checks, Linux packaged-binary acceptance, CodeQL, cargo-audit and Dependency Review. Missing, queued, cancelled or failed checks are not passing checks. Preserve unsuccessful attempts and diagnose the cause rather than weakening tests.

## CI and dependency assessment

The existing workflow already covers Windows x64, Linux x64, macOS ARM and macOS Intel. It verifies the pinned macOS stripping runtime, retains bounded per-test/process checks and uploads JUnit plus native acceptance evidence. Its Linux native acceptance invokes the packaged executable against local CLI/OAuth/MCP fixtures, including earlier SSE regressions. The added Cargo test files are automatically discovered, and `scripts/check.ps1` verifies that every tracked test suite appears in Cargo metadata. Do not create a redundant second test workflow for this correction.

RC01 introduces no dependency or lockfile change. Existing daily Dependabot coverage for Cargo and GitHub Actions remains sufficient for this patch; do not duplicate it or force an unrelated SDK migration. Evaluate Python native-fixture dependency tracking separately: a version embedded in a workflow is not a normal pip manifest. An optional locked test requirements file and matching Dependabot pip entry can be considered after checking the owner's Rules and supply-chain policy, not presented as a confirmed runtime defect.

## Reviewer's complete same-session execution contract

### 1. Rules, research and Spec Kit

Read actual applicable Rules/hooks first. Run the complete ordered Spec Kit chain they prescribe, including their research/specification/plan/task/implementation/verification/integration artifacts and receipts. Do not guess custom command names or mark a hook executed from this document. Research current official documentation and relevant upstream GitHub code; record sources and decisions at the locations required by the Rules. Understand the deployed dependency pins before considering a migration.

Start with a clean inventory of main, the PR head, open PRs, local changes, configured hooks and installed binary provenance. Preserve unrelated local work. Check whether a finding is already integrated by content and tests, not only by branch or PR status. Reuse the existing PR branch unless the Rules genuinely require another branch; do not open competing repairs for RC01.

### 2. Commit identity — mandatory for past and future publication

Every published author and committer email must be `Kiaro.Sama.Dev@gmail.com`, including prior history and all new commits, as the owner requires. Inspect raw `%ae` and `%ce`, not only a web attribution label. The first repair commit already has both required raw fields, verified from Git metadata. The audited baseline ancestry also reported only the required raw email. Temporary connector-created staging objects used a noreply identity, then were replaced on the isolated staging ref before PR publication; they are not ancestors of the proposed fix. Main, release tags and global account settings were not rewritten by the auditor.

If any nonconforming published history remains, immediately plan and execute its safe correction under the actual Rules: create and verify a private recovery bundle, record exact old refs and an old-to-new mapping, coordinate dependent branches/tags, preserve intended trees/topology and provenance, handle signatures as the Rules require, and use explicit expected-ref leases. Do not silently drop other contributors' work, force-push blindly or assume a mailmap/coauthor trailer replaces a raw identity repair. Rerun exact-head checks after a history rewrite. Use this identity for future commits and final integration as well.

### 3. Verify the implemented change rather than rewrite it from scratch

Review the PR diff and the two original red-to-green test failures. Confirm that only refresh lock contention changes policy; generic configuration locking and all previous safety contracts stay intact. Run the helper and integration tests, including the cross-process and route-cancellation extension, on the final head. Test invalid destinations, cancellation, killed owners, rotation and separate roots, not just uncontested acquisition. Do not substitute an infinite lock wait or broaden credentials/network permissions.

Use PowerShell 7 for the owner's Windows review. Resolve the actual checkout below `G:\Program Files\Portable\Scripts`; do not assume the path spelling of an existing install. Keep temp, toolchain/cache and evidence paths project-contained using the repository's existing scripts. When writing any additional script, provide timestamped INFO/WARNING/ERROR/DEBUG-capable logging, English comments and no secrets. Do not dump tokens, proxy passwords, authorization URLs/states or the entire environment.

Run the repository's existing check/verify/build/package commands, with native exit codes checked. Typical individual commands are `pwsh -NoProfile -File ./scripts/check.ps1`, `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo nextest run --locked --all-targets --profile ci --no-fail-fast`; invoke them through the existing project-contained verification setup required by the Rules. Use exact final SHA and tree in every receipt.

### 4. Complete all PR dispositions and integration

Review every open PR according to the owner's Rules; do not exempt PRs by a blanket unrelated-PR instruction. Merge each eligible, fully verified PR using the required identity and workflow. When a PR cannot be merged directly, fix its code/conflicts yourself, implement and verify the corrected equivalent, integrate it with traceable provenance, then close the superseded PR as required by the Rules. Closing without implementing/validating the intended correction is not DONE.

Fix newly reproduced bugs encountered along the way in this same session, add regression tests and run affected plus full required suites. Do not defer a known code failure to another audit round. Conversely, do not reopen a previously fixed item without a failing current-head reproduction. Evaluate optional improvements separately and record an explicit accept/reject/defer decision rather than pretending they are verified defects or mandatory release blockers.

Only after Rules/hooks, all required final-head checks and review are satisfied, commit/push remaining approved changes and merge any necessary feature branches. Verify integrated main and the actual installed/package binary separately. Preserve private authentication/proxy/client state and previous releases. Resolve all PRs and tasks under the Rules and publish a final evidence-backed completion receipt. A genuine external permission, runner or live-account prerequisite must be stated accurately; never fabricate completion to make a task list empty.

## Optional improvements, not confirmed additional defects

Consider an upstream-compatible single-flight refresh design only with a precise contract for near-expiry refresh versus explicit 401-driven refresh, token rotation and separately running processes; skipping every guarded refresh could hide revoked tokens. The implemented correctness fix intentionally retains the pinned SDK behavior.

Consider safe diagnostic categories for lock-wait duration and timeout without recording credential paths or token values. Confirm the normal logging/privacy contract and avoid excessive polling logs.

Consider scheduling the existing broader concurrency/native acceptance suite periodically if it gives useful signal. Keep retries disabled for assertions, preserve artifacts and avoid redundant CI matrices. Evaluate these ideas freely with the owner; record the decision instead of inflating this repair into an open-ended redesign.

## Primary references

- [Pinned rmcp 3.5.1 CredentialStore coordination and token refresh source](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs)
- [Tokio timeout, yielding and cancellation semantics](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html)
- [Rust File::try_lock and cross-process ownership](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
- [Git commit-tree author/committer identity](https://git-scm.com/docs/git-commit-tree)
- [Git recovery bundles](https://git-scm.com/docs/git-bundle)
- [Git explicit force-with-lease publication](https://git-scm.com/docs/git-push#Documentation/git-push.txt---force-with-leaseltrefnamegtltexpectgt)
- [Baseline CI and native acceptance pipeline](https://github.com/KiaroSama/Upwork-Agent-Plugin/blob/810844fc68ed23d80e8969b1a80d9d65011d3bec/.github/workflows/ci.yml)
- [Implemented PR #12 and current verification receipts](https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/12)

Final acceptance requires an evidence-backed resolution of RC01 and all PR dispositions under the actual Rules. The auditor leaves PR merge/close authority with the owner and reviewing agent.
