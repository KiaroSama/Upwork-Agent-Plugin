> Historical engagement record, not current instructions or authorization. Directives below describe original review requirements only; they grant no current execution, publication, merge or history-rewrite permission. Evidence remains baseline-specific. See [current dispositions](README.md).

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

The original review required the applicable private Rules/hooks and their Spec Kit sequence, with primary-source research on collection entry semantics and the import contract. The public report did not establish private-hook execution or supply custom command names.

Its initial inventory covered main, the repair PR, then-open PRs, local changes and installed/package provenance. Credentials, registrations, proxy settings, unrelated work and prior releases were preserved. The engagement preferred its existing repair branch unless governing Rules required otherwise; an interrupted response/download was not grounds to duplicate the implementation.

### Identity and historical publication

The original engagement required `Kiaro.Sama.Dev@gmail.com` in both raw fields of owner-attributed commits, verified through `%ae`/`%ce` rather than web labels, mailmaps or trailers. Confirmed nonconforming owner history required verified private recovery, mappings, unchanged-content checks, coordinated refs/tags and explicit expected-ref leases under the governing Rules. Rewrites invalidate signatures and affect descendant/release SHAs; provenance was retained. This records that historical procedure, not current rewrite authorization.

The repository recorded an explicit owner decision allowing authentic bot identities and retaining a signed Dependabot commit. That exception resolved H01 without rewriting legitimate bot attribution. This audit did not rewrite main or certify unseen private Rules. Temporary preparation helpers were excluded from the proposed repair ancestry; new owner-attributed proposed commits used the required owner email.

### Verify the implemented correction, not a replacement design

The original review compared the full diff and before/after evidence against exact first-free naming. Deterministic complexity/differential tests remained load-bearing; benchmarks alone could not prove absence of quadratic work. Duplicate suffix handling, skipped-record stability and failed-import atomicity were required contracts.

Its Windows procedure used PowerShell7, the actual existing checkout and contained temp/cache/bounded wrappers. Additional scripts required English comments and timestamped safe diagnostics; credentials, authorization links/states, private bodies and environment dumps were excluded.

The historical gate included source inventory, formatting, strict Clippy, bounded all-target tests, build/launcher/cleanup/package and security checks. Recorded commands through the then-current environment were:

```powershell
pwsh -NoProfile -File ./scripts/check.ps1
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo nextest run --locked --all-targets --profile ci --no-fail-fast
```

That engagement required native exit-code checks, final Windows/Linux x64 and macOS ARM64/Intel jobs, Linux packaged import-scaling acceptance, RustSec, CodeQL and Dependency Review. Manual dispatch with reviewed `base_ref` and one final batch was retained. Failed evidence, assertion integrity and exact-head attribution remained requirements; these archived commands do not authorize present local execution.

### Finish all required work and PR dispositions

The original engagement required repair/regression checks for newly reproduced defects, not open-ended expansion from unsupported hypotheses or rejected redesigns. That historical acceptance criterion does not expand a later task.

The original engagement required review and disposition of all its then-open PRs under the owner's Rules. Eligible changes required independent review and verification; supersession required an equivalent verified correction with traceable provenance. A closed PR alone was not completion evidence. The auditing assistant performed no merge or closure, and this historical text authorizes neither for a later session.

Historical closure required Rules/hooks, identity handling and final checks before approved commit/push/integration, with separate main and installed/package verification. One finding ledger and evidence-backed receipt recorded actual outcomes and external permission/account prerequisites. This is not present Git or deployment authorization, nor a claim that every possible scenario was tested.

## CI, Dependabot and optional maintenance

No runtime or development dependency upgrade is needed for IM01. Cargo.toml and Cargo.lock remain unchanged; the standard library is sufficient. Daily Cargo/GitHub Actions Dependabot remains. Existing optional hashed Python requirements/pip updates and aggregate lock telemetry are maintenance decisions, not reopened runtime defects.

The optional maintenance proposal was to document import scale and retain representative native timings as diagnostic artifacts, without tight platform-specific thresholds. GPU offload was rejected for this small stateful name-allocation operation; removing redundant work avoided new deployment dependencies.

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
