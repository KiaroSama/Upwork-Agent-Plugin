Before doing any work, read and fully obey the owner's applicable private Rules, repository instructions and hooks. Follow the complete, ordered Spec Kit chain defined in those Rules, including the required research and verification records. Do not invent replacement Rules or claim that unpublished hooks were executed.

# Credential coordination audit — continuation and integration handoff

Repository: [KiaroSama/Upwork-Agent-Plugin](https://github.com/KiaroSama/Upwork-Agent-Plugin)  
Date: 2026-10-15  
Existing implementation PR: [PR #12](https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/12)  
Default-branch baseline: `810844fc68ed23d80e8969b1a80d9d65011d3bec` (`2.1.3`)  
Continuation baseline: `d387b624440eacb59f8eddb093c126b51310ca99` (the already-open PR #12)

## Scope, authority and non-repetition

Continue PR #12; do not create a competing refresh implementation or another PR for its already-fixed contention behavior. The previously merged SSE terminal-response correction in PR #11 and the earlier initialization-envelope, callback, CLI/import and registration fixes are not reopened by this continuation.

The auditing assistant may publish/update this PR but must not merge or close any PR or issue, update main, publish releases, replace the user's installed binary, or modify private account state. The owner's reviewing agent must complete the integration and all PR dispositions under the actual Rules after the required checks pass.

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

Read the actual Rules/hooks first and execute the exact Spec Kit sequence defined there, including research, specification, plan, tasks, implementation, verification and integration records as applicable. Do not guess custom command names. Research current official upstream documentation and relevant GitHub implementations, especially the pinned SDK's credential-store, refresh and code-exchange contracts, and record decisions where the Rules require.

Inventory current main, PR #12's actual head, every open PR, local modifications, configured hooks and installed/package provenance before integration. Preserve unrelated local work and all private authentication/proxy/client files. Reuse the existing branch unless the Rules establish a genuine need for another. Determine whether a fix is already integrated from its code and tests, not from a title alone.

### 2. Required identity for all past and future publication

Every published author and committer email must be `Kiaro.Sama.Dev@gmail.com`, for prior history and all future commits. Inspect raw Git `%ae` and `%ce`; a web attribution label, mailmap or coauthor trailer is not a substitute. Verified outgoing commits must use both required raw fields before they are attached to the PR.

Temporary connector-created staging helper objects are not part of the proposed commit ancestry. Canonical commits are created with the required author and committer, and temporary helper workflows are excluded from the proposed tree. The assistant does not rewrite main or alter global account settings.

If a nonconforming historical publication is found, immediately correct it under the owner's Rules: retain and verify a private recovery bundle; record exact old refs and an old-to-new mapping; coordinate affected branches/tags and signatures; preserve intended trees, topology and provenance; use explicit expected-ref leases; and reverify the rewritten head. Never blindly force-push or silently discard another writer's changes. Final integration commits must use the same required identity.

### 3. Verify both writer directions and the actual final binary

Review the implemented diff and the before/after receipts rather than rewriting the solution from scratch. Confirm that both refresh and manual callback use one shared cross-process transaction lock, while save does not recursively acquire it. Verify invalid destinations, timeout, cancellation, terminated owners, separate installation roots, token rotation, failed/forged callbacks and preserved private state.

Use PowerShell 7 on the owner's Windows system. Resolve the existing checkout beneath `G:\Program Files\Portable\Scripts` without inventing an installation path or moving files. Keep temporary files, caches and test evidence project-contained through the repository's existing setup. Any additional script must have English comments and timestamped INFO/WARNING/ERROR/DEBUG-capable logging. Do not log passwords, tokens, authorization URLs/states, private request bodies or the entire environment.

Run the existing project check, formatting, strict Clippy, bounded all-target tests, native build/launcher/cleanup/package checks and security gates. Relevant commands include `pwsh -NoProfile -File ./scripts/check.ps1`, `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo nextest run --locked --all-targets --profile ci --no-fail-fast`, invoked using the repository's project-contained environment and the actual Rules. Check every native exit status.

Require final Windows x64, Linux x64, macOS Intel and macOS ARM results; Linux packaged acceptance must include the new coordination probe plus every previous suite. Require CodeQL, cargo-audit and Dependency Review. Preserve failed and interrupted attempts. Diagnose causes rather than adding assertion retries, ignoring tests, weakening safety checks or calling cancelled jobs successful.

### 4. Finish every PR disposition and newly found defect in the same session

Review every open PR according to the owner's Rules; do not exempt PRs with a blanket unrelated-PR instruction. Merge each eligible, independently reviewed and fully verified PR. When direct merging is unsuitable, fix the code/conflicts yourself, implement and verify the correct equivalent, integrate it with traceable provenance, and then close the superseded PR as the Rules require. Closing a PR without its intended correction implemented and verified is not DONE.

Fix any newly reproduced defect encountered during review in the same session, add regression coverage and run the affected and full required checks. Do not leave a known code failure for the next audit. Conversely, do not reopen a previously resolved item without a failing current-head reproduction. Optional improvements require an explicit accept/reject/defer decision and must not be mislabeled as confirmed defects.

After all Rules/hooks, final-head checks and review pass, commit and push remaining approved work, merge any necessary feature branch, and verify integrated main and the actual installed/package artifact separately. Preserve previous releases and private state. Resolve all PRs and tasks under the Rules and issue one evidence-backed completion receipt. A genuine external prerequisite must be named accurately, not hidden by a fabricated success.

## CI, Dependabot and optional improvements

Daily Dependabot already covers Cargo and GitHub Actions. This correction introduces no new dependency, so do not add a duplicate updater or force an unrelated SDK migration. The existing matrix and native runner are extended rather than replaced by a redundant test pipeline.

Evaluate moving the native fixture's pinned Python dependencies into a hashed requirements file with a corresponding pip Dependabot entry. The version currently embedded in a workflow is not a normal pip manifest. This is a supply-chain maintenance improvement, not a reproduced runtime defect in this audit.

Evaluate safe lock-wait duration/timeout diagnostic categories without credential paths or token values. Avoid poll-by-poll logging. Evaluate upstream-compatible single-flight refresh only with separate contracts for near-expiry refresh and explicit 401-driven refresh; skipping every guarded refresh can mask token rejection. Neither optional redesign is required to close RC01/RC02.

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
