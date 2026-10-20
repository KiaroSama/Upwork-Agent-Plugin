# Same-session CI follow-up for PR #7

> Historical engagement record, not current instructions or authorization. Imperatives below describe original requirements only; they do not authorize current tests, publication, merges or history rewrites. See [current dispositions](README.md).

## Archived engagement requirements and evidence

The original engagement required the owner's private Rules/hooks and Spec Kit chain before review or integration. This addendum extended the sign-in verification audit in the same PR without reopening merged PR #5. Its auditor had no merge/close/main authority. The original reviewer was responsible for then-open PR dispositions, owner-attributed raw identity correction to `Kiaro.Sama.Dev@gmail.com`, reproduced-defect repairs and final integrated evidence. These historical responsibilities grant no present action authority.

## Evidence on the application-fix head

Application-fix head: `25e884dfc626e1194df1a30e986cac61f2a02927`, content tree `42136346fe9b621383509f7b48de9fba3095527c`. CI run [38449774048](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38449774048) tested synthetic merge `a50e7f492c8b8c6aa8623828279b637c85316397`; its tree equals the PR head tree.

Downloaded and checksum-verified JUnit reports contain 358 passing compiled test executions: Linux 92, Windows 84, macOS Intel 91 and macOS ARM 91; zero failures, errors or skips. Every platform executed all 15 new verification tests. The final Linux executable also passed the 30-case native sign-in matrix three complete times and 272 CLI assertions in 119 child processes. Those remain valid receipts for that application content, but do not make an aggregate cancelled workflow successful.

## C01 — insufficient cold macOS job budget

The macOS ARM job started at `2026-10-13T18:59:16Z` and ended at `19:19:43Z`, or 20 minutes 27 seconds. Its workflow budget was 20 minutes. All reported steps, including tests, package smoke, artifact uploads and cleanup, completed successfully, yet the job and aggregate run concluded `cancelled`. The cancellation timing is consistent with the configured job timeout; the cancellation annotation itself was not available through the connector, so no particular cancellation actor is asserted.

The elapsed cold lint, test and optimized release builds leave no headroom under that budget. Increase only macOS job budgets to 30 minutes; retain 20 minutes on Windows/Linux and the existing 5-minute dependency-audit job. This is a bounded orchestration allowance, not a relaxation of assertions, individual test deadlines, process wall/idle bounds, retries or security gates. Do not solve this by repeated reruns until one happens to finish quickly.

## C02 — missing stripping-tool runtime prerequisite

The same ARM release log reported `rust-objcopy` aborting with `Library not loaded: @rpath/libLLVM.dylib` at `19:19:10Z`. Cargo continued with a warning and produced an executable that passed package smoke. This is a build-tool/runtime prerequisite failure, not evidence that the application's runtime failed or that credentials were compromised. A successful Cargo exit is not proof that requested stripping completed.

The pinned Rust 1.99.0 distribution source explicitly includes target LLVM runtime libraries in the optional `llvm-tools` component. Install that component along with rustfmt/clippy on the two macOS runners, and execute the exact sysroot `rust-objcopy --version` before the expensive builds. The preflight is a required, two-minute-bounded step with timestamped INFO/WARNING/ERROR/DEBUG-capable diagnostics and no credential/environment dump. Missing files, unexpected host metadata or a loader failure block the job. Do not use a random PATH tool, disable stripping, alter global loader/trust settings, or mark failure as tolerated.

## Acceptance and boundaries

This follow-up changes only CI plus this audit addendum. The V01/V02 Rust implementation, its 15 tests, dependency lockfile and runtime semantics remain byte-identical to the tested application-fix head. This supersedes the earlier audit's statement that CI is entirely unchanged: its assertions and security checks are unchanged, but macOS budget and toolchain preparation are intentionally corrected here.

The original acceptance gate required the new head/tree, exact-tool preflight on both macOS runners, absence of the earlier ARM loader/stripping warning, and an overall `success` conclusion. Individual successful steps or uploaded artifacts were insufficient. Four-platform packaging/security and final Linux native/CLI receipts were distinct from the earlier three stability runs; failed/cancelled evidence was retained.

Owner-attributed commits were subject to raw identity verification because the connector API could not set author/committer email. Mailmaps/trailers did not replace that check. Historical remediation required private recovery, unchanged content, coordinated leases and replacement-head verification. The auditor did not rewrite main or change account settings; this paragraph grants no current rewrite authority.

## Primary references

- [GitHub Actions job timeout semantics](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idtimeout-minutes)
- [Rustup components](https://rust-lang.github.io/rustup/concepts/components.html)
- [Rust 1.99.0 LLVM tools distribution, including target runtime libraries](https://github.com/rust-lang/rust/blob/1.99.0/src/bootstrap/src/core/build_steps/dist.rs#L2687-L2710)
- [macOS ARM job and complete release log](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38449774048/job/115477377503)
- [Implemented sign-in findings and original acceptance contract](2026-10-13-sign-in-verification.md)
