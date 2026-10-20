# 2026-10-13 recheck: bounded proxy-file imports

> Historical engagement record, not current instructions or authorization. Imperatives below describe original requirements only; no current execution, publication, merge or history rewrite is authorized. Evidence is baseline-specific. See [current dispositions](README.md).

## Archived engagement requirements and evidence

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

The original final-head gate required inventory, formatting, strict Clippy, bounded tests, launcher/cleanup/package checks, CodeQL, Cargo audit and Dependency Review. Daily Cargo/Actions Dependabot was retained; this standard-library fix required no dependency update. Weakened gates and assertion retries were not accepted evidence. This archived gate is not current local-execution authorization.

The original engagement required `Kiaro.Sama.Dev@gmail.com` in owner-attributed raw author/committer fields, inspected through Git objects rather than trailers. Its connector selected a noreply address without an override. The reviewing agent rebuilt only those five owner commits, preserving every content tree, parent topology, date and message, with verified private recovery. The rewritten head was `476e71ec1cda2c857f239d4164277d92fde3899e`. Publication used an exact observed-ref lease; replacement-head CI remained a separate integration gate. Recovery, coordination and exact leases were historical repair safeguards, not authority for a later agent to rewrite history. Legitimate bot attribution remained distinct. No global account settings or main history were changed by the auditor.

The original engagement required primary-source research, the owner's private workflow, review of the then-open PRs, and verified integration or documented supersession. A closed PR alone was not evidence of a corrected defect. Existing-branch reuse and integration gates described that engagement only; this report grants no later merge, closure, branch or publication authority. The auditing assistant did not merge or close PRs.

## Primary references

- https://doc.rust-lang.org/std/fs/fn.read.html
- https://doc.rust-lang.org/std/io/trait.Read.html#method.take
- https://doc.rust-lang.org/std/fs/struct.Metadata.html#method.is_file
- https://pubs.opengroup.org/onlinepubs/9799919799/functions/open.html
- https://docs.github.com/en/actions/how-tos/monitor-workflows/use-workflow-run-logs
- https://github.com/github/spec-kit
