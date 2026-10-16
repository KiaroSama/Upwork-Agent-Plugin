Before doing any work, read and fully obey the owner's actual private Rules, repository instructions and hooks. Follow the complete ordered Spec Kit chain prescribed by those Rules, including required research, specification, plan, tasks, implementation, verification and integration records. Never invent substitute Rules or claim that unpublished hooks were executed.

# Upwork Agent Plugin — bulk import scaling audit and review handoff

Date: 2026-10-16. Repository: https://github.com/KiaroSama/Upwork-Agent-Plugin

Reviewed baseline: `fe707e9639f19a41f95b46eaeec05edc4f97c939`, version `2.1.5`, tree `42164a0f3155a925e1faea4effa061dfe82c0421`.

Implementation branch: `fix/import-name-scaling-2026-10-16`. The corresponding implemented-fix PR, exact final head and verified final CI receipt are linked in the delivery. Reuse that PR once opened; do not create a competing implementation. A staged branch or a document alone is not a claim that a PR exists.

## Non-repetition and authority

PR #15 is merged into this baseline. Its initialization-ping handling, negotiated-version boundary, bounded state reads/writes, dangling-link refusal, complete UTF-8 validation and mixed initialization-envelope rejection are retained. Prior credential-writer coordination, callback, registration and CLI/input corrections are not reopened without a failing current-head reproduction. The baseline's existing successful CI is run `38894257305`; it is historical evidence, not a replacement for final changed-head checks.

This recheck isolates one algorithmic performance defect, IM01. Timing measurements describe the exercised inputs and runner; they are not portable speed guarantees. No new authentication or filesystem-race defect is claimed from speculation.

The assistant may open/update implemented repair PRs but must not merge/close PRs or issues, rewrite main, publish releases, modify installed binaries or touch private user state. The owner's reviewing agent performs integration and resolves every PR under the actual Rules. The local execution service returned errors; newly collected executable and compilation evidence is explicitly GitHub-hosted, not an owner's-machine or local-container execution claim.

## IM01 — quadratic name allocation for colliding proxy labels

### Impact and root cause

`src/import.rs::import_file` normalizes each display label to an ASCII identifier and assigns the first unused suffix. For every imported proxy, the old implementation restarted at the unsuffixed base and then scanned suffixes from 2, even after earlier records had already filled that prefix.

With N distinct proxy URLs sharing one normalized base and no pre-existing conflicting names, the old loop performs exactly `N * (N + 1) / 2` occupied-name checks. With 24,000 records that is 288,012,000 checks; a legal input can remain below the existing 1 MiB import limit. The configuration lock remains held during allocation, so needless CPU work also lengthens contention for other configuration commands.

This affects not only literally repeated display names. Distinct labels can normalize to the same ASCII slug, including labels consisting entirely of non-ASCII characters, and long labels can collide after the existing 54-character truncation. Rejecting those labels, randomly renaming profiles or lowering the input limit would be a compatibility regression, not a proper fix.

### Implemented correction

`src/import/names.rs::Names` owns the used-name set and a separate next-candidate cursor for each normalized base. Names only accumulate during one transaction. Once a candidate has been found occupied, it cannot become free later in that transaction, so the cursor can safely advance without rescanning.

The first allocation still considers the unsuffixed base, then suffix 2, suffix 3 and so on. Existing holes are filled exactly as before. Nested bases such as `pool` and `pool-2`, case-sensitive bases, and numeric spellings such as `p-02` retain the old behavior. Independent imports create independent allocators; cursor state is neither global nor persisted. A checked counter overflow returns a safe error without publishing a partial configuration.

Only name allocation changes in the production importer. URL validation/normalization, duplicate detection, display-label parsing, slug generation, input size limit, ordering, active-route preservation and atomic configuration publication stay in their existing locations. Duplicate URLs are filtered before reserving a name, so they do not consume suffixes.

For one repeated base, the new allocator makes one candidate-set insertion attempt per new record, plus a one-time scan of any initially occupied prefix. The overall import still performs parsing, validation and serialization; do not describe the entire application or arbitrary hash-table worst cases as constant time.

### Regression strategy

Six allocator tests include an independent copy of the old first-free rule as a reference. All 64 subsets of a small initial-name universe and all 81 four-record base sequences are compared, covering 5,184 separate histories. Dense prefixes, holes, nested bases, case and numeric spellings, independent imports and safe overflow are also checked.

The complexity gate is deterministic: 50,000 allocations of a single fresh base must require exactly 50,000 candidate probes. Another test adds 8,000 records after an 8,000-name occupied prefix and requires 16,000 total probes, not repeated rescans. Counters are test-only and add no production telemetry or credential logging. This is stronger and less flaky than a narrowly timed assertion on a shared runner.

Three integration tests execute the actual importer. They cover URL duplicates not consuming names, gaps and nested labels, exact output order, selected-route preservation, idempotent reimport, cross-import continuation, non-ASCII slug fallback, long-label collisions, malformed trailing input with unchanged configuration, and a 4,096-record import/reimport round trip.

`tests/native/import_scale_probe.py` executes the real compiled CLI against synthetic local files. It verifies every resulting name, retained labels, record counts, source preservation, exact duplicate reimport behavior and the active route. It never connects to any listed proxy. It records bounded wall/CPU timings and safe counters, not proxy passwords or input payloads. The retained native acceptance runner adds this suite once; every previous suite remains enabled.

### Before/after experiment

The baseline binary is taken from successful exact-source CI `38894257305`. Its archive checksum and release metadata source SHA are checked before execution. The same probe is then run on the corrected release build, on the same hosted runner with the same 2,000-, 8,000- and 24,000-record fixture sizes. Every file must fit the 1 MiB import limit. Timing measurements are retained even though correctness and operation-count gates, not an arbitrary speed ratio, govern acceptance.

The preparation job appends only verified measurements. Final cross-platform CI and packaged-binary acceptance are a separate requirement and must be tied to the actual final head, not the earlier baseline or preparation snapshot.

## Required reviewing-agent completion contract

### Rules, Spec Kit and research

Read the actual private Rules/hooks first, then follow the exact Spec Kit sequence defined there. Research official documentation and relevant upstream GitHub implementations, especially standard collection entry semantics and the repository's import contract. Record findings and decisions in the locations required by the Rules. Do not invent custom command names or treat this public report as proof that private hooks ran.

Read current main, the repair PR, all open PRs, local modifications and installed/package provenance before changing anything. Preserve private credentials, registrations, proxy settings, unrelated work and previous releases. Reuse the existing repair branch unless there is a genuine Rules-governed reason for another branch. Do not duplicate a fix merely because a previous response or download was interrupted.

### Identity and historical publication

All previous and future published author and committer identities must follow the owner's requirement for `Kiaro.Sama.Dev@gmail.com`. New owner-attributed commits must have both raw fields set to that address; inspect `%ae` and `%ce`, not only web attribution, mailmaps or trailers. Correct any actually nonconforming history immediately under the governing Rules, with a verified private recovery bundle, old-to-new mapping, unchanged-content checks, coordinated refs/tags and explicit expected-ref leases. Rewriting invalidates original signatures and affects descendant/release SHAs; never misrepresent rewritten signatures or silently discard provenance.

The current repository records an explicit owner decision allowing authentic bot identities and retaining a signed Dependabot commit. Reconcile the generic email requirement with the actual Rules and that recorded owner decision before any historical rewrite. Do not reopen historical H01 solely by ignoring a legitimate Rules-approved bot exception. This audit neither rewrites main nor certifies unseen private Rules. Temporary preparation helpers are excluded from the proposed repair ancestry; new proposed commits use the required owner email.

### Verify the implemented correction, not a replacement design

Review the complete diff and before/after evidence. Preserve the exact first-free naming behavior. Keep deterministic complexity and differential tests; do not replace them with a benchmark that can pass despite quadratic behavior. Ensure duplicates do not reserve suffixes, skipped records do not rename existing entries, and failed imports do not publish a partial state.

Prefer PowerShell 7 on Windows. Locate the actual existing checkout under `G:\Program Files\Portable\Scripts`; do not invent or move the installation directory. Use the repository's project-contained temp/cache setup and bounded execution wrappers. Every additional script must use English comments and timestamped INFO/WARNING/ERROR/DEBUG-capable logs. Never emit tokens, passwords, authorization links/states, private request bodies or an environment dump.

Run the existing source inventory, formatting, strict Clippy, all-target bounded tests, build/launcher/cleanup/package checks and security reviews. Relevant commands, through the established environment and wrappers, are:

```powershell
pwsh -NoProfile -File ./scripts/check.ps1
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo nextest run --locked --all-targets --profile ci --no-fail-fast
```

Check every native exit code. Require final Windows x64, Linux x64, macOS ARM64 and Intel jobs, the packaged Linux acceptance suites including import scaling, RustSec, CodeQL and Dependency Review. Keep production CI manually dispatched with explicit reviewed `base_ref`; do not restore automatic push/PR triggers or weaken checks. Use one final complete batch after the code is ready. Keep unsuccessful evidence, diagnose real failures and do not retry assertions until green or borrow another head's successful counts.

### Finish all required work and PR dispositions

Fix any newly reproduced defect encountered during review in the same session, add its regression and finish the affected plus full required checks. Do not leave a known code failure for another audit. Equally, do not turn an unsupported hypothesis or rejected optional redesign into a fresh defect.

The reviewing agent must review and resolve ALL PRs according to the actual Rules. Merge eligible independently reviewed and verified PRs. When direct merging is unsuitable, correct the code/conflicts yourself, implement and verify an equivalent solution, integrate with traceable provenance, and only then close superseded PRs as those Rules require. Closing a PR without its intended verified correction is not DONE. The auditing assistant does not perform any merge or close.

After Rules/hooks, required identity handling and final checks pass, commit/push remaining approved changes, merge any necessary feature branch, and verify integrated main and the actual installed/package binary separately. Maintain one finding ledger and one evidence-backed completion receipt. A real external permission or live-account prerequisite must be named, not hidden behind a claim that every possible scenario has been tested.

## CI, Dependabot and optional maintenance

No runtime or development dependency upgrade is needed for IM01. Cargo.toml and Cargo.lock remain unchanged; the standard library is sufficient. Daily Cargo/GitHub Actions Dependabot remains. Existing optional hashed Python requirements/pip updates and aggregate lock telemetry are maintenance decisions, not reopened runtime defects.

Consider documenting expected import scale and keeping representative native measurements as diagnostic artifacts. Do not add platform-specific tight timing thresholds. GPU offload is not appropriate for this small, stateful name-allocation operation; removing redundant work is the relevant optimization and avoids new deployment dependencies.

## Primary references

- Reviewed importer: https://github.com/KiaroSama/Upwork-Agent-Plugin/blob/fe707e9639f19a41f95b46eaeec05edc4f97c939/src/import.rs
- Baseline CI: https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38894257305
- Rust HashMap Entry: https://doc.rust-lang.org/std/collections/hash_map/enum.Entry.html
- Rust HashSet insertion semantics: https://doc.rust-lang.org/std/collections/struct.HashSet.html#method.insert
- Manual GitHub workflow dispatch: https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow
- Git commit identity: https://git-scm.com/docs/git-commit-tree
- Recovery bundles: https://git-scm.com/docs/git-bundle
- Explicit push leases: https://git-scm.com/docs/git-push

## Evidence boundary

Source review, a reproduced scaling measurement, deterministic algorithm tests, full CI, local installed-state verification and live-provider behavior are different claims. Report each accurately. This change does not make any claim about real-account Upwork operations, unpublished hooks or the user's installed binary.

## Verified hosted preparation measurements

Strict Clippy and **174 compiled Linux tests** passed with no failures/errors/skips. The deterministic linear-probe and differential-history tests were present in the actual JUnit report. Baseline and corrected native runs passed every naming, duplicate and active-route assertion.

| Records | Input bytes | Baseline seconds | Corrected seconds | Observed ratio |
|---:|---:|---:|---:|---:|
| 2000 | 62000 | 0.107796 | 0.009082 | 11.87x |
| 8000 | 248000 | 1.881448 | 0.029315 | 64.18x |
| 24000 | 759000 | 19.80266 | 0.087202 | 227.09x |

These are same-runner real-binary measurements, not guaranteed speeds. The old algorithm produced correct names but did unnecessary quadratic work; no baseline correctness assertion is falsely described as failing. Final cross-platform and packaged-binary results remain a separate receipt.
