> Historical engagement record, not current instructions or authorization. All directives below describe original review requirements only; they grant no execution, publication, merge, account or history-rewrite permission. Evidence remains baseline-specific. See [current dispositions](README.md).

# Credential coordination audit — continuation and integration handoff

Repository: [KiaroSama/Upwork-Agent-Plugin](https://github.com/KiaroSama/Upwork-Agent-Plugin)  
Date: 2026-10-15  
Existing implementation PR: [PR #12](https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/12)  
Default-branch baseline: `810844fc68ed23d80e8969b1a80d9d65011d3bec` (`2.1.3`)  
Continuation baseline: `d387b624440eacb59f8eddb093c126b51310ca99` (the already-open PR #12)

## Scope, authority and non-repetition

The continuation reused PR #12 rather than creating a competing refresh implementation. Previously merged SSE terminal-response work in PR #11 and earlier initialization-envelope, callback, CLI/import and registration fixes stayed outside its reopened scope.

The original auditor's authority covered updates to that PR, not merges, closures, main, releases, installed binaries or private account state. Integration and disposition of its then-open PRs belonged to the owner/reviewer after the applicable checks. These historical role boundaries grant no current publication or integration permission.

The local container execution service returned errors before any new local compilation could run. New execution evidence in this continuation comes from GitHub-hosted runners. Native tests execute real compiled connector processes against synthetic local TLS/OAuth services. No request is forwarded to Upwork; no real account login, proposal, message, purchase or Connects expenditure occurs. Disposable trust is limited to child environments, not the host trust store.

## Findings and implemented corrections

### RC01 — healthy concurrent refresh is treated as a storage failure

**Status:** already implemented in the existing PR; retained, not rewritten or rediscovered as a new bug.

The baseline `Store::acquire_refresh_guard` used the generic fail-fast `Lock::acquire`. A second OAuth manager sharing an installation could receive `CredentialStoreError` while a valid first refresh owned the lock. The existing private `src/credentials/refresh_lock.rs` helper polls only `Lock::try_acquire` at 50 ms intervals with one total 60-second deadline. Invalid/linked/non-file/I/O failures still fail immediately. No detached blocking worker, lock-file deletion, network fallback or token-request replay is introduced.

The pinned rmcp SDK reloads credentials after acquiring the guard. It may perform two serialized refresh exchanges, with the second using the newly rotated refresh token. This is coordination, not a claim of single-flight deduplication. Ordinary configuration locking remains fail-fast.

The original before/after run is [38752019023](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38752019023). The existing PR's later head also completed [CI 38754808392](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38754808392) before the continuation. Those earlier results are not substituted for final-head verification.

### RC02 — an old refresh can overwrite a successfully completed manual sign-in

**Priority:** P2 correctness and session integrity. This is a distinct writer-coordination defect within the same credential lifecycle, addressed in the same PR rather than deferred to another audit.

**Root cause:** the SDK's `refresh_token` acquires the credential-store refresh guard across load, token exchange and save. Its authorization-code exchange does not acquire that guard. The application's manual callback previously called `session.handle_callback_with_issuer(...)` directly. Atomic file replacement prevents torn JSON but does not prevent this stale transaction ordering.

**Native reproduction on the existing PR head:**

1. Seed an isolated root with expired synthetic credentials and the saved local callback registration.
2. Start a real `bridge` process and submit an initialize request. The local token endpoint gates the resulting refresh after observing it.
3. Start a real `sign-in` process against the same root and submit its actual generated callback using the in-memory state and issuer.
4. Before releasing the refresh, observe successful manual exchange, callback completion and the sign-in process's success claim.
5. Release the older refresh and allow it to save. Its credentials replace the freshly authorized manual session.

The fixture deliberately lets manual sign-in finish before releasing the older refresh in the vulnerable case. It does not infer the overwrite merely from timing or manufacture it by writing credentials itself. A separate two-bridge control verifies that RC01's existing serialized-refresh fix remains effective.

**Implemented repair:** `src/auth/callback_transaction.rs::finish` acquires the same `Store::acquire_refresh_guard` before invoking the SDK callback and retains it through the entire authorization-code exchange and persistence. `src/auth.rs` routes the actual manual callback through that helper. The existing refresh helper and its deadline are unchanged.

If an older refresh owns the guard, manual code exchange waits until that transaction finishes; then the manual session is saved last. If a code exchange is already in progress, a competing refresh waits and reloads the newly saved credentials. Do not acquire the same lock inside `CredentialStore::save`, because the SDK already calls save while holding its refresh guard and that would deadlock.

Invalid issuer/state callbacks still do not exchange tokens. Exchange failures do not replace the previous credentials. Dropping a callback future releases ownership; outer route-change and sign-in deadlines continue to apply. Lock polling is the only repeated operation introduced by RC01; neither change automatically replays token or marketplace requests.

## Added regression coverage

The continuation adds six Rust test functions around the actual callback transaction and pinned SDK: waiting before manual exchange/save; retaining ownership through a gated token operation; cancellation without credential publication; failed exchange preserving state; forged-issuer rejection with a still-usable valid callback; and two managers reloading a rotated token under serialized refresh.

The new `tests/native/credential_coordination_probe.py` executes two independent real-process cases: manual sign-in overlapping an older refresh, and two overlapping bridges. Its receipt contains only fixed case names, booleans and request counts, never tokens, authorization links/states, private headers or certificate keys. Every process, fixture wait and callback has a deadline, and owned children are reaped.

`tests/native/run.py` includes this probe once in the existing Linux packaged-binary acceptance runner. All previous CLI, protocol, extended-envelope and SSE-ordering suites remain enabled. The existing four-platform Rust matrix automatically executes the callback tests; no runtime dependency, lockfile, generic lock policy or version change is needed.

The before/after continuation runner is [38768210614](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38768210614). Its unmodified-production build uses the existing PR head, not a reconstructed older implementation. Its baseline contract requires RC02 to reproduce while RC01 continues passing. The repaired build must preserve the manual session and retain both successful bridge initializations with correctly rotated refresh credentials. Compilation failure, fixture setup failure, a queued job or an available archive alone is not an accepted passing result.

The measured receipts appended below are from that hosted before/after execution. Final four-platform CI and packaged acceptance must be tied to the final PR head and content tree in the PR's completion receipt; do not borrow a pass from an earlier SHA. A GitHub synthetic PR merge ref is not an integration into main.

## Complete reviewing-agent execution contract

### 1. Rules, Spec Kit, current research and working-state inventory

The original engagement required the applicable Rules/hooks and their Spec Kit sequence, with research, specification, design, tasks and verification/integration records. Its primary-source research covered the pinned SDK's credential-store, refresh and code-exchange contracts. The public report neither supplied private custom command names nor established that unavailable hooks ran.

The original integration inventory covered main, PR #12's actual head, then-open PRs, local modifications, configured hooks and installed/package provenance. Its preservation constraints included unrelated work and private authentication/proxy/client files. Branch reuse was preferred unless governing Rules required otherwise, and integration was determined from code/tests rather than titles.

### 2. Required identity for all past and future publication

The original engagement required `Kiaro.Sama.Dev@gmail.com` for owner-attributed author/committer fields and inspection of raw Git `%ae`/`%ce`, rather than relying on web labels, mailmaps or trailers. Its outgoing owner-attributed commits were subject to that gate; legitimate bot attribution was not a request to impersonate the owner.

Temporary connector-created staging helper objects are not part of the proposed commit ancestry. Canonical commits are created with the required author and committer, and temporary helper workflows are excluded from the proposed tree. The assistant does not rewrite main or alter global account settings.

The original proposed remediation for confirmed nonconforming owner publication required verified private recovery, exact ref/mapping records, coordinated tags/signatures, preserved trees/topology/provenance, explicit expected-ref leases and rewritten-head verification. It excluded discarding another writer's work. This historical procedure authorizes no present rewrite and does not replace legitimate bot identities with the owner's address.

### 3. Verify both writer directions and the actual final binary

The original review compared the implemented diff and before/after receipts rather than replacing the design. Both refresh and manual callback shared one cross-process transaction lock; save remained nonrecursive. Acceptance covered invalid destinations, timeout, cancellation, terminated owners, separate roots, rotation, failed/forged callbacks and private-state preservation.

Its Windows procedure used PowerShell7 and the actual existing checkout, with project-contained temporary files/caches/evidence. Additional scripts required English comments and timestamped safe diagnostic levels. Secrets, authorization URLs/states, private bodies and environment dumps were excluded. These historical constraints did not authorize installation relocation or disclosure.

The engagement's verification gate included the existing project inventory, formatting, strict Clippy, bounded all-target tests, native build/launcher/cleanup/package checks and security checks. Its recorded commands included `pwsh -NoProfile -File ./scripts/check.ps1`, `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo nextest run --locked --all-targets --profile ci --no-fail-fast`, through the then-current contained environment with native exit-status checks.

Its final acceptance required Windows x64, Linux x64, macOS Intel/ARM, all previous Linux packaged suites plus coordination, CodeQL, cargo-audit and Dependency Review. Failed/interrupted attempts were retained; retries, ignored tests, weakened safety or cancelled jobs were not substitutes for success. These are historical gates, not instructions to execute those commands locally today.

### 4. Finish every PR disposition and newly found defect in the same session

The original engagement required disposition of its then-open PRs under the owner's Rules: independently reviewed and verified integration, or a traceable equivalent correction before supersession. A closed PR without its intended verified correction did not satisfy that historical acceptance criterion. This paragraph grants no authority over present or unrelated PRs.

The original engagement required same-session repair/regression coverage for newly reproduced defects and current-head reproduction before reopening resolved items. Optional improvements received separate accept/reject/defer decisions rather than being labeled confirmed defects. This recorded acceptance criterion is not an instruction to expand a later task.

Historical closure required approved commit/push/integration only after Rules/hooks, final-head checks and review, followed by separate main and installed/package verification. Previous releases and private state were preservation requirements. The original completion receipt accounted for its then-open PRs/tasks and genuine external prerequisites; it did not authorize present Git or installed-state changes.

## CI, Dependabot and optional improvements

Daily Dependabot already covers Cargo and GitHub Actions. This correction introduces no new dependency, so do not add a duplicate updater or force an unrelated SDK migration. The existing matrix and native runner are extended rather than replaced by a redundant test pipeline.

The original optional proposal was a hashed Python requirements file with a pip Dependabot entry, replacing a workflow-embedded version. A maintained pinned requirements manifest and pip updater were later added; complete transitive hash locking remains unclaimed. This was maintenance, not a reproduced runtime defect.

Optional lock-wait duration/timeout categories excluded credential paths, token values and poll-by-poll noise. The single-flight proposal depended on separate near-expiry and explicit401 refresh contracts because skipping guarded refresh can mask token rejection. Neither redesign was required for RC01/RC02 and neither was adopted by this audit.

## Primary references

- [Pinned rmcp CredentialStore and refresh/code-exchange implementation](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs)
- [Tokio timeout and cancellation](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html)
- [Rust File::try_lock](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
- [Git commit-tree identity](https://git-scm.com/docs/git-commit-tree)
- [Git recovery bundles](https://git-scm.com/docs/git-bundle)
- [Git force-with-lease](https://git-scm.com/docs/git-push#Documentation/git-push.txt---force-with-leaseltrefnamegtltexpectgt)
- [Existing PR #12 and its exact-head review receipts](https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/12)

The assistant leaves merge/close authority with the owner and reviewing agent. Technical test success does not certify execution of unavailable private Rules/hooks or compatibility with every live account/provider condition.

## Hosted before/after receipt — verified during continuation

Code commit: `3a0d26342e6269e2d1163ecffd92d334d65b2cf5`. Code tree: `83a22a5e9e756e513c957b21dd814c41767040f1`.

The continuation runner completed strict Clippy, source inventory, diff checks and **148 compiled Linux tests with zero failures/errors/skips**. The two native coordination cases passed on the corrected executable.

| Observed contract | Existing PR before callback fix | Corrected code |
|---|---|---|
| Manual exchange occurs while the old refresh is held | Yes — defect | No |
| Completed manual session survives the old refresh | No — overwritten | Yes |
| Both overlapping bridges initialize and reload rotated credentials | Yes | Yes |

The final documentation-only commit does not alter these code/test files. Its final four-platform run and exact PR head are recorded in the PR receipt; this Linux receipt does not stand in for other platforms.
