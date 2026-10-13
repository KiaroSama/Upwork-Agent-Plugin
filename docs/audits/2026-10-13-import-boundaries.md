# 2026-10-13 recheck: bounded proxy-file imports

First obey the owner's actual private Rules, hooks, and prescribed Spec Kit chain. This report does not replace them or claim their execution. The reviewing agent, not the auditing assistant, owns final integration and PR disposition.

## Baseline and non-duplication

Baseline is `43b75110177bf0420b45e95a2ec965df4bcfd9ee`. PR #2 was merged on 2026-10-12; its A01-A09 fixes are present, and current main also adds the MCP registration rename, diagnostics and heartbeat recovery. Do not reopen those nine findings simply because this is another audit. The implemented import correction is in PR #5: https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/5 . A separate artifact contains the before/after recheck ledger and final-head CI receipts.

## Reproduced defects

R01: `import_file` calls `fs::read` before enforcing its documented 1 MiB cap. A 64 MiB sparse file under a 64 MiB Linux address-space limit produces exit code 1 with the generic `Cannot read proxy file. Check the path, permissions and UTF-8 encoding.` message, rather than the intended size error. The existing configuration remains unchanged, but the cap does not protect memory/read demand. This is a read/allocation failure, NOT an observed process abort. The first draft used the word abort incorrectly; the captured exit code and diagnostic were rechecked and the description corrected before handoff.

R02: opening a named pipe before validating the input type waits indefinitely for a writer. A synthetic no-writer FIFO exceeded the independent harness deadline and was killed/reaped. The CLI promises a file import, not a streaming pipe reader.

Both belong to one import-boundary fix, not a regression of the earlier registration/routing changes. These are local robustness defects, not a claim of remote account compromise.

## Implemented solution

Validate regular-file type and declared size before opening; revalidate the opened handle; independently restrict actual reading to 1 MiB plus one sentinel byte. Reject overflow before UTF-8 parsing, record construction, configuration locking or publication. Preserve exact-limit valid inputs, BOM handling, quoted paths, duplicate suppression, atomic import, original input bytes, and symlinks to regular source files. Error messages never expose file contents, credentials or raw I/O errors.

The streamed bound is mandatory even with metadata validation: metadata alone does not handle a file growing after inspection. The pathname precheck rejects an ordinary FIFO before its blocking open, but does not claim race-free protection against an attacker swapping filesystem objects between syscalls. Use a private trusted source directory; network-filesystem latency is not bounded by this change. Allocation is bounded by a fixed multiple of the input cap, not a claim that allocator capacity is exactly 1 MiB.

## Regression gates

Four unit tests cover bounded consumption with a longer reader, exact/empty reads, interrupted/short reads and safe error messages. Six Linux integration tests cover the file-size boundary, malformed inputs, sparse oversized files, address-space-constrained CLI execution, no-writer FIFO rejection and supported regular-file symlinks. Platform-specific tests use conditional compilation: FIFO/symlink on Unix; address-space limit on Linux. Every spawned import has a four-second kill/reap deadline. Existing Cargo test discovery runs this suite in the unchanged four-platform CI matrix.

Independent baseline binary recheck: 27 passing controls and two reproduced failures out of 29 cases. No new-source success is asserted by this document until final-head CI and post-fix artifact testing provide it. Do not confuse a successful baseline workflow with a successful fix workflow. The initial PR-head CI stopped at two rustfmt layout differences in the new test file; those were corrected, not waived or retried without a change. The final verification receipt distinguishes the old and corrected heads.

## Review and completion

Run inventory validation, formatting, strict Clippy, all bounded tests, launcher/cleanup/package checks, CodeQL, Cargo audit and Dependency Review on the exact final head. Keep daily Cargo/Actions Dependabot; no dependency update is needed for this standard-library fix. Do not weaken gates or retry deterministic test failures into apparent success.

All published author and committer emails must be `Kiaro.Sama.Dev@gmail.com`. Inspect actual Git objects, not commit-message trailers. The connector selected a noreply address and exposes no override; this is an explicit unresolved integration gate, not a completed identity correction. Audit earlier history and every PR commit, preserve a recovery copy, coordinate repair under the owner's Rules, and use exact leases rather than a blind force push. No global account settings or main history were changed by the auditing assistant.

Research primary documentation and record decisions in the Rules-prescribed location. Review every PR according to those Rules. Fix newly reproduced defects in the same session; merge eligible PRs after all gates, or manually integrate/correct the necessary changes and close superseded PRs with evidence. The assistant must neither merge nor close. Do not equate closed with fixed, or DONE with merely opened. Prefer the existing working branch; do not create extra branches without need.

## Primary references

- https://doc.rust-lang.org/std/fs/fn.read.html
- https://doc.rust-lang.org/std/io/trait.Read.html#method.take
- https://doc.rust-lang.org/std/fs/struct.Metadata.html#method.is_file
- https://pubs.opengroup.org/onlinepubs/9799919799/functions/open.html
- https://docs.github.com/en/actions/how-tos/monitor-workflows/use-workflow-run-logs
- https://github.com/github/spec-kit
