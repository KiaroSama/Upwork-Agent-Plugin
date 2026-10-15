Before doing any work, read and fully obey the owner's actual private Rules, repository instructions and hooks. Execute the complete Spec Kit chain in the order specified there, including required research, specification, plan, tasks, implementation, verification and integration records. Do not invent commands or claim that unpublished Rules/hooks were executed.

# Persisted JSON state boundary audit — 2026-10-16

Repository: https://github.com/KiaroSama/Upwork-Agent-Plugin

Baseline: `8c075c7b08a3de48ea13a3439a28309d0fa542dc`, version `2.1.4`, tree `ac17d215e647d82fcece37543ecb3d33ddecc414`.

## Scope and previously completed work

PR #12 has been merged. Its RC01/RC02 credential-transaction fixes and the reviewing agent's reverse-order/scheduling tests are preserved. No previously resolved initialization, SSE, registration ownership or import finding is reopened. This audit examines a different reachable input boundary: persisted JSON consumed by normal configuration, routing and credential loading, rather than the explicit proxy-import file.

The implementation is proposed on the single state-input audit PR created from this exact baseline after before/after verification. The final PR description and downloadable receipt identify its number, canonical head/tree and execution outcomes. Do not integrate a staging helper or a tests-only intermediate commit.

## SI01 — persisted special files can block synchronous reads indefinitely

`config::read`, `settings::read` and `credentials::optional_json` used `fs::read_to_string` directly. A writerless FIFO placed at a persisted-state path blocks `open`; a device or growing file can prevent a complete read. These functions are also called from a current-thread Tokio runtime. A timer or route-change future on the same thread cannot cancel a synchronous call that never returns.

Affected callers include `status`, `local-settings`, bridge token loading, current/legacy client registration and legacy token loading, and the route watchdog reading `connections.json`. Registration snapshots had a separate regular-destination preflight but still read an unrestricted amount of bytes.

This is a local-state availability and fail-closed correctness issue, not a claim that a remote Upwork server can create local FIFOs. A malformed state path must fail with a controlled error instead of hanging a connector or its revocation watchdog.

## SI02 — unbounded internal JSON reads and ambiguous absence

The existing import bound does not bound persisted connection aggregates or credential JSON. Direct filesystem reads allocate/read the complete input before parsing. Also, a dangling symlink was treated as a missing file through `NotFound`, silently selecting default settings or permitting legacy fallback despite an invalid existing state entry.

The new policy is explicit: truly absent state retains the old default/None behavior; an existing dangling link is an error; ordinary links to regular files remain supported. Persisted JSON has a generous **16 MiB inclusive** limit, separate from the unchanged **1 MiB** import limit. The shared writer checks the same limit before creating directories or temporary files, so the application cannot publish state that its own reader refuses. Existing oversized files are preserved for operator review, never silently truncated or deleted. Do not work around rejection by discarding profiles or credentials.

## Implemented correction

`src/storage/state_input.rs` provides the shared optional-byte reader. It checks pathname type/length before opening, uses `O_NONBLOCK` on Unix so a FIFO substituted between pathname validation and open cannot block in open, validates the opened descriptor, and reads at most limit plus one byte. The descriptor check and actual byte bound remain even after metadata checks. Only truly missing paths return None.

`config.rs`, `settings.rs`, `credentials.rs` and registration snapshots use this helper. Their JSON schemas, fixed safe diagnostics, route identities, token locking, callback validation, explicit route selection and no-fallback behavior are unchanged. `storage::atomic_bytes` checks the write limit before any publication side effect. Failed updates preserve original content and avoid leftover temporary files.

A Unix-only direct dependency on `libc` supplies the platform's `O_NONBLOCK` constant without application unsafe code or hardcoded OS numeric flags. The crate was already in the lockfile; every resolved external version and checksum is preserved. The dependency-review job must verify that precise change, not approve a broad dependency upgrade.

The fix does not promise a deadline on defective disks or arbitrary network filesystems, a complete defense against an administrator continuously replacing directory components, or a cap on the whole parsed object graph/process RSS. It bounds the byte input and rejects the reproduced special-file cases. Valid regular-file links and aggregate configuration larger than one import are explicitly retained.

## Practical before/after and regression contract

The isolated hosted audit first compiles unchanged baseline production files and runs `tests/native/state_input_probe.py --expect-baseline`. The native matrix covers FIFO and linked FIFO for configuration/local settings/credentials; dangling links and oversized valid JSON for configuration/local settings; and missing, regular, linked-regular and >1 MiB aggregate positive controls. Only the explicitly expected baseline outcomes count as reproduction; a compiler/setup failure is not a reproduced bug.

After applying the correction, the same real-process matrix must reject every invalid state promptly, preserve the input entry and reap every child. The positive controls must remain successful. The probe uses only temporary project roots and a loopback-only refusal proxy for bridge tests; nothing is forwarded to Upwork. It exports case names, booleans, exit codes and durations, not credential bodies or authorization links.

Rust tests cover exact byte limits, one-byte overflow, a counting endless/growing reader, writer-side preservation, missing/default behavior, invalid JSON, regular and dangling links, FIFO/device descriptor checks, every current/legacy credential path, and revocation after a live route's configuration is replaced with a FIFO. Unix-only tests use their actual platform facilities; platform-specific absence is not misreported as a skipped failing test.

All prior native suites remain in `tests/native/run.py`; the state-input probe is added once. Require formatting, strict Clippy, bounded all-target tests, native launcher/cleanup/package tests, the full packaged Linux acceptance matrix, RustSec, CodeQL and explicit base/head Dependency Review on the exact final head. Preserve the manual-only main CI configuration. Do not restore automatic push/PR CI triggers or start redundant batches simply to obtain a green result.

The remote staging helper is a one-shot verification/publication mechanism, not a shipped workflow. It is excluded from the proposed tree/ancestry. Its canonical commit uses the required raw author and committer identity and explicitly dispatches the existing manual CI once on the verified source. Staging success is not substituted for the final cross-platform batch. The local execution service was unavailable; hosted execution must be labeled as such, not claimed as local or installed-machine testing.

## Reviewing-agent completion instructions

1. Read the actual Rules/hooks first and follow their exact Spec Kit chain. Research current official upstream documentation and relevant GitHub implementations; record source links, decisions, verification and rejected alternatives where the Rules require. Preserve private files, local work and the prior audit ledger.
2. Inspect the current main and the existing implementation PR's exact head. Reuse its branch; create another only when genuinely required by the Rules. Do not create a competing repair for SI01/SI02 or reopen a merged finding without a failing current-head reproduction.
3. All historical and future raw author AND committer emails must be `Kiaro.Sama.Dev@gmail.com`. Inspect `%ae` and `%ce`, not attribution labels. Correct any nonconforming history immediately under the Rules with a verified private recovery bundle, old-to-new/ref mapping, coordinated signature/tag handling and explicit force-with-lease expectations. Preserve intended trees and provenance; never blindly overwrite another writer. A mailmap or trailer is not a history repair. The auditing assistant does not rewrite main.
4. Review the code and test evidence, including both baseline and fixed native outcomes. Verify all listed caller paths, the read/write ceiling consistency, metadata and descriptor checks, default-versus-invalid semantics and zero secret disclosure. Verify no private state is truncated or reset. Fix every newly reproduced defect encountered in this session with a regression test rather than leaving it for another audit.
5. Use PowerShell 7 for the owner's Windows workflow and locate the actual existing checkout under `G:\Program Files\Portable\Scripts`; do not invent or move its installation. Use the repository's cache/temp preparation. Every new script must have English comments and timestamped INFO/WARNING/ERROR/DEBUG-capable logging with no credentials, callback states or environment dumps. Relevant checks include `pwsh -NoProfile -File ./scripts/check.ps1`, `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo nextest run --locked --all-targets --profile ci --no-fail-fast`, through the actual Rules/environment. Check native exit codes.
6. Review **all PRs according to the Rules**, without a blanket unrelated-PR exemption. Merge eligible independently reviewed and fully verified PRs. If direct merging is unsuitable, implement and verify the correct equivalent, integrate it with traceable provenance and then close superseded PRs as the Rules require. Closing without delivering the correction is not DONE. Commit/push approved remaining work, merge necessary feature branches only after all gates pass, and verify integrated main plus the installed/package binary separately. Resolve all tasks/PR dispositions with one evidence-backed completion receipt. The auditing assistant may open/update PRs but must never merge or close PRs/issues.

## CI / Dependabot and optional improvements

Keep the existing daily Cargo and GitHub Actions updates and manual batch orchestration. The reviewing agent already deferred hashed Python fixture requirements/pip updates and extra lock telemetry as optional maintenance; do not relabel those as new runtime bugs. Consider a configuration-size diagnostic before an import would exceed the persisted limit, with transactional no-write behavior and no profile-data disclosure. Consider read-handle filesystem hardening only with explicit supported-platform and compatibility requirements, not an unbounded redesign.

## Primary references

- Rust filesystem reads: https://doc.rust-lang.org/std/fs/fn.read.html
- Rust Read::take and read-to-end behavior: https://doc.rust-lang.org/std/io/trait.Read.html#method.take
- Unix OpenOptionsExt::custom_flags: https://doc.rust-lang.org/std/os/unix/fs/trait.OpenOptionsExt.html#tymethod.custom_flags
- Tokio timeout requires yielding: https://docs.rs/tokio/latest/tokio/time/fn.timeout.html
- Git commit identity: https://git-scm.com/docs/git-commit-tree
- Git recovery bundles: https://git-scm.com/docs/git-bundle
- Baseline source: https://github.com/KiaroSama/Upwork-Agent-Plugin/tree/8c075c7b08a3de48ea13a3439a28309d0fa542dc

A passing synthetic suite is evidence for its contracts, not proof that all possible live accounts, hosts, filesystems or future changes have been tested. Final publication and installed-state verification remain subject to the owner's Rules and reviewing authority.

## Audit-fixture correction retained in the evidence ledger

The first isolated run, 38824442349, reproduced all three direct FIFO stalls but rejected its own linked-FIFO assertions. Its fixture had constructed relative link targets from a relative temporary root. The corrected fixture resolves the temporary root, verifies actual target/type before launch, checks nondefault content in positive controls and hashes regular inputs for preservation. The failed first attempt is retained; no application assertion was weakened or retried until green. The staging editor also avoids a Python 3.11-only TOML import on the Ubuntu 22 runner; external lockfile blocks are compared byte-for-byte.
