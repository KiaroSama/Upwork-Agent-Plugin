Before doing any work, read and fully obey the owner's applicable private Rules, repository instructions and hooks. Follow the complete ordered Spec Kit chain defined by those Rules, including research, specification, planning, tasks, implementation, verification and integration records. Do not invent substitute Rules or claim unpublished hooks were executed.

# Upwork Agent Plugin — N01/N02 implemented repair and H01 integration handoff

Audit date: 2026-10-16. Repository: https://github.com/KiaroSama/Upwork-Agent-Plugin

Reviewed baseline: `8c075c7b08a3de48ea13a3439a28309d0fa542dc` (2.1.4). Baseline tree: `ac17d215e647d82fcece37543ecb3d33ddecc414`.

Implementation branch: `fix/initialization-ping-state-bounds-2026-10-16`. Implemented repairs are published in [PR #15](https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/15). Review and complete this existing PR rather than creating a duplicate. The final PR conversation records the exact reviewed head and complete CI outcome.

## Scope and authority

Continue the three identified items N01, N02 and H01. PR #12 was merged before this pass; retain its refresh lock, manual callback transaction and reverse writer-order tests. Earlier CLI/import, registration rollback, raw initialization envelope and first-terminal SSE fixes are not reopened. Do not recategorize intentionally manual application CI as a bug.

The assistant is authorized to open/update repair PRs, not merge/close any PR or issue or rewrite main. The owner's reviewing agent handles integration and all PR dispositions under the actual Rules. Preserve private credentials, client registrations, proxy settings, installed executables and unrelated local work. No live Upwork login, proposal, message, purchase, Connects spending or host trust-store modification is authorized by these synthetic tests.

Local container/Python execution failed before application execution. New experimental evidence comes from the isolated GitHub-hosted audit runner and the final manually dispatched application CI, not the owner's machine. Staging-only preparation helpers are excluded from the proposed tree and ancestry; the application CI's manual dispatch policy is retained. The runner dispatches the final existing CI once after canonical publication with explicit baseline input.

## N01 — ping-gated SSE initialization deadlock

### Root cause and reproduction

`src/http_route/initialization_response.rs::first_terminal` consumed SSE server requests while waiting for a terminal result/error. It discarded PingRequest without a reply. Both ordinary bridge and strict manual sign-in traversed this selector. MCP allows pings before initialization completes and requires an empty response carrying the request's ID.

The deterministic native fixture sends a ping and does NOT release its initialize result until the connector supplies the matching empty result. If no answer arrives, the fixture's bounded gate expires and closes its stream. The unchanged baseline must fail this case in both sign-in and bridge while a no-ping control succeeds. An immediate initialize result after a ping is not sufficient evidence. The fixture uses actual compiled processes and synthetic loopback CONNECT/TLS only.

### Implemented correction

`src/http_route/initialization_control.rs` retains the first-terminal behavior but handles pre-initialization pings. Its context captures the selected reqwest client, original endpoint, authorization, request headers and session fallback before the initialize POST consumes them. It sends an empty result over the same routed client, using the response session when supplied, and bypasses the SDK initialization queue so there is no circular wait.

The raw wire ID is preserved, including numeric strings such as `"007"`. Do not reconstruct it from an SDK-normalized ID. No initialize request is replayed, no request is forwarded to another origin, no direct fallback is used, and no premature `notifications/initialized` is generated. The server must accept a ping result with HTTP 202; other statuses and failures remain errors, without automatic application-level retry.

Each control POST has a 5-second deadline. There are at most 16 pings and 256 pre-initialization events. The complete initialization POST/selection has a single 30-second deadline in the common guarded path. Yield points preserve cancellation responsiveness when events are already buffered. The existing route guard cancels a blocked/stale route, including a stalled control response. Unsupported pre-initialization requests fail safely; they are never executed as business tools.

Existing schema/version/response-ID verification and error-before-success selection remain. Normal post-initialization relay and heartbeat behavior are not redesigned. No SDK upgrade or transport downgrade is required.

### Regression coverage and acceptance

Rust tests exercise both internal transport modes with numeric/string/numeric-string IDs, session/auth/custom/protocol headers, multiple pings, finite ping budget, rejected control publication and Block while a reply is stalled. The exact initialization request count is one.

The native ping probe exercises nine cases in each of sign-in and bridge: numeric ID, string ID, numeric-string ID, multiple pings, preceding log, no-ping control, ping followed by terminal error then success, rejected reply, and ping overflow. All 18 cases must satisfy their expected positive or negative contract. A negative protocol case passing means the connector rejected it correctly, not that it signed in.

Keep all existing raw-envelope, wrong-ID/version, terminal-error, framing, no-replay and route-cancellation tests. Require actual before/after receipts and the final packaged Linux probe results before closing N01.

## N02 — blocking and unbounded internal-state reads

### Root cause and reproduction

`config::read`, `settings::read` and `credentials::optional_json` used unbounded synchronous whole-file reads. `register::snapshot` bounded neither snapshot bytes nor publication size, although registration already checked destination types. A FIFO at an internal path could hang the current-thread runtime; a valid excessively large registration document could be rewritten without any limit. Async timeouts alone cannot preempt a synchronous filesystem operation that never yields.

The native baseline probe independently exercises FIFO-without-writer at connections, local-proxy settings and native credentials paths in owned child processes with external deadlines and reaping. Credential tests select only a loopback proxy, so no request can reach Upwork. A separate oversized valid `.mcp.json` case checks that publication is rejected without changing its original checksum.

### Implemented correction and compatibility policy

`src/storage/bounded.rs` implements one regular-file reader with a shared 16 MiB state-file ceiling. It validates pathname metadata, validates the opened handle, and limits real consumption to ceiling plus one byte. On Unix, safe `OpenOptionsExt` with the already-locked libc version's O_NONBLOCK prevents a FIFO substituted between preflight and open from blocking in open. Registration additionally rejects leaf links/reparse points using its existing policy and the no-follow handle path.

Existing regular-file links remain readable for configuration, settings and credential paths where they were previously accepted. Registration remains stricter. Proxy import behavior and its separate 1 MiB per-input limit are unchanged. The new state ceiling permits multiple imports to accumulate beyond 1 MiB. Pre-existing state larger than 16 MiB is refused, never truncated or rewritten; an operator must back it up and reduce it explicitly. This bounded-state contract is not a claim that 16 MiB is a provider-defined limit.

Serialization is bounded as well. `storage::json_bytes` uses a capped writer, includes the final newline in the limit and does not first allocate an unlimited serialized output. `atomic_json` uses it; `atomic_bytes` rejects oversized data before creating temporary files or modifying a destination. Read and write limits therefore agree.

Registration computes the MCP serialization before publishing its denial settings, and settings use the same bounded serializer. Preserve denial-first order, ownership checks, exact-byte concurrent-edit detection and rollback. No recursive credential lock is added to save. Missing-file defaults and native/legacy credential behavior are retained.

One Unix-only direct libc dependency is added for portable OS flag constants. The registry version/checksum already in Cargo.lock is reused; no transitive dependency version is intentionally updated. Require the locked dependency diff and security review to confirm this.

### Regression coverage and acceptance

Unit tests cover exact-limit and one-byte-over reads, limit-plus-one counted consumption, interrupted/short reads, missing/empty files, nonregular paths, invalid UTF-8 preservation for caller validation, ordinary symlink compatibility, registration no-follow, and post-preflight nonblocking FIFO validation. A successful serialized state larger than 1 MiB must read back. Oversized serialization/raw writes must preserve the existing destination bytes.

The native probe checks FIFO, directory, oversized and invalid-UTF-8 input for the three direct readers plus oversized valid registration: 13 cases. Every fixed case must terminate within its external deadline with a controlled failure and no credential dump. Registration's original bytes must survive rejection.

These measures do not promise to interrupt a stuck kernel, a stalled network filesystem or every hostile concurrent parent-directory replacement. Keep that boundary explicit. Do not replace bounded reads with detached blocking tasks and call the problem solved.

## H01 — integrated Dependabot commit has nonconforming raw identities

This is a verified publication-policy mismatch, not an application defect. Commit `95a95f1e4e7ba52fd6ff53f306331ea7a323532f` entered reviewed main through PR #13 ancestry. Its raw author is `49699333+dependabot[bot]@users.noreply.github.com`; its raw committer is `noreply@github.com`. The main tip itself uses the requested email. The bot commit has an original verified signature.

ALL prior and future published author and committer emails must be `Kiaro.Sama.Dev@gmail.com`, according to the owner's Rules. New proposed commits are generated with both raw fields set to that address. This does not retroactively repair H01: a normal descendant PR, mailmap or coauthor trailer cannot alter an ancestor's raw identities. Do not claim otherwise.

The authorized reviewing agent must immediately handle history repair under the actual Rules: record current branch/tag SHAs; create and verify a private recovery bundle; retain the original signed commit and provenance; construct an old-to-new mapping; change the required identity fields without changing content trees; coordinate affected refs/tags and releases; and publish only with explicit expected-ref leases. Rewriting changes commit IDs and invalidates original commit signatures. Never call a rewritten bot commit still signed by GitHub, remove provenance silently, or blindly force-push.

The auditing assistant does NOT rewrite main. H01 remains an integration action for the authorized reviewer. Recheck raw `%ae` and `%ce` on the final integrated ancestry and every later publication; protect future bot integration according to the Rules rather than knowingly reintroducing the same mismatch. Signed tags and release/source receipts must be revalidated after history repair.

## One-pass reviewing-agent execution contract

Read the actual Rules/hooks first, then execute their complete Spec Kit sequence. Research current official primary documentation and relevant GitHub implementations, particularly MCP initialization/control messages, pinned SDK transport behavior and safe regular-file access. Save research, decisions, specification, tasks and test receipts where the Rules require. Unpublished Rules and hooks were not executed by this audit.

Re-read current main, the implementation branch, all open PRs and local dirty state. Reuse the existing N01/N02 PR. Do not create a competing patch, reopen merged RC01/RC02 without a failing reproduction, or apply an old patch to a different head blindly. If current code has moved, reconcile the exact diff while preserving others' work.

Review the implemented code, not only this document. Match each finding to a failing baseline oracle, its correction and successful regression/packaged-binary receipt. If you discover a real additional bug during review, fix it and add a regression in the SAME session rather than deferring a known failure to another audit. Conversely, reject unsupported hypotheses instead of expanding the work indefinitely.

Prefer PowerShell 7 on Windows. Locate the actual checkout under `G:\Program Files\Portable\Scripts`; do not invent or move the installation folder. Use the repository's project-contained cache/temp configuration and wrappers. New scripts require English comments and timestamped INFO, WARNING, ERROR and DEBUG-capable logging. Never log passwords, tokens, authorization links/states, private headers or full environment dumps.

Required checks through the existing environment include:

```powershell
pwsh -NoProfile -File ./scripts/check.ps1
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo nextest run --locked --all-targets --profile ci --no-fail-fast
```

Inspect every native exit status. Require the final manual application CI batch on the exact reviewed head with explicit `base_ref`, all four platform tests/builds/launchers/cleanup/packages, Linux packaged acceptance, RustSec, CodeQL and Dependency Review. Preserve previous failed attempts. Do not skip tests, add assertion retries until green, weaken security gates, or borrow an earlier-head pass. A staging test is distinct from final application CI; an artifact existing is not proof of success.

Once code is ready, complete one final batch rather than many speculative full batches. An isolated preparation run may compile before/after to establish evidence; it is not a replacement for final cross-platform certification. Preserve the intentional manual dispatch policy in the production workflow.

Review and resolve ALL PRs according to the owner's Rules. Merge an eligible independently reviewed and verified PR. If direct merging is unsuitable, correct the code/conflicts, integrate a verified equivalent with traceable provenance, and then close superseded work according to the Rules. Closing without integrating the intended verified correction is not DONE. Do not exempt all unrelated PRs from the owner's explicit all-PR disposition rule.

After Rules/hooks, H01 identity handling and final checks pass, commit/push approved changes, integrate any necessary feature branch, and verify both integrated main and the actual installed/package executable. The assistant leaves merge/close authority with the owner/reviewing agent. Keep one ledger with N01, N02 and H01, exact evidence and final dispositions. Name genuine external blockers accurately instead of claiming an impossible guarantee.

## CI, dependency maintenance and optional improvements

The normal application CI remains manually dispatched. The existing native runner adds the two new probes once and retains the previous six suites, including the owner's third credential-coordination case. Rust tests are automatically discovered across Windows x64, Linux x64, macOS ARM64 and Intel. Do not duplicate the matrix.

Keep daily Cargo/GitHub Actions Dependabot. A hashed Python fixture requirements file with pip Dependabot and aggregate lock-wait diagnostics remain optional maintenance decisions, not reopened runtime defects. Do not invent a single-flight token shortcut without separate expiry and 401 contracts. Evaluate structured pre-initialization control diagnostics without exposing raw IDs, headers or payloads.

## Primary references

- MCP initialization lifecycle: https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle
- MCP Ping contract: https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/ping
- Pinned SDK transport: https://github.com/modelcontextprotocol/rust-sdk/tree/rmcp-v3.5.1/crates/rmcp/src/transport
- Tokio timeout/cancellation: https://docs.rs/tokio/latest/tokio/time/fn.timeout.html
- Rust OpenOptions: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html
- Rust Read::take: https://doc.rust-lang.org/std/io/trait.Read.html#method.take
- Linux FIFO behavior: https://man7.org/linux/man-pages/man7/fifo.7.html
- Git raw commit identity: https://git-scm.com/docs/git-commit-tree
- Recovery bundles: https://git-scm.com/docs/git-bundle
- Explicit push leases: https://git-scm.com/docs/git-push
- Baseline source: https://github.com/KiaroSama/Upwork-Agent-Plugin/tree/8c075c7b08a3de48ea13a3439a28309d0fa542dc
- Historical baseline CI: https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38815842518

## Verification receipt

The preparation runner appends only measurements it actually verified. The final application CI result and actual repair PR URL are recorded in the delivery and PR conversation. Nothing here certifies a live account, the owner's installed binary, unavailable private hooks, or completion of H01 history repair.

### Hosted before/after verification

The unchanged baseline reproduced N01 in both sign-in and bridge and N02 in three FIFO reader paths plus oversized valid registration. The corrected build passed strict Clippy, source checks and **160 compiled Linux tests with zero failures/errors/skips**, plus **18 native gated-ping cases** and **13 native state-reader cases**. Full original JSON/JUnit/build logs are retained. These results are not a claim of final cross-platform CI or H01 history repair.


### Same-session publication correction

The preliminary manual CI run [38829787614](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38829787614) correctly failed source inventory: two generated Python `.pyc` files were inadvertently staged after the preparation check. This was an audit-publication mistake, not another original application defect. Both files are removed, bytecode paths are ignored, and direct invocation of the new ping probe disables sibling bytecode. The final staged inventory is validated AFTER all reviewed files are staged and staging helpers are excluded.

The tested Rust source and dependency files are unchanged. The unmerged PR snapshot is replaced only under an exact expected-head lease after retaining a verified recovery bundle and old-to-new mapping, so generated bytecode does not enter the proposed commit ancestry. Main and other refs are not rewritten. Original failed-run evidence remains visible. A second final manual batch is necessary for the corrected publication; it is not a retry of unchanged failed assertions. New commit author and committer remain `Kiaro.Sama.Dev@gmail.com`; inherited H01 is explicitly still a separate owner-authorized history action.


### N01 compatibility completion: offered is not negotiated

A same-session review added a deterministic server that gates initialization on a ping and then selects supported protocol `2025-03-26` instead of the client's offered `2025-11-25`. Candidate `3eda8f3b5d604183a271bef902947423b6341773` incorrectly attached the offered version to its ping response before negotiation, causing that older server to reject the control POST. This is a correction to this PR's N01 implementation, not a reopened unrelated finding.

The final handler preserves an explicitly supplied protocol header exactly once but does not invent one from InitializeRequest before InitializeResult. After successful negotiation, the existing SDK sets the negotiated header on readiness/subsequent traffic. The native fallback case verifies both a successful ping-gated fallback and the negotiated readiness header. This follows the TypeScript SDK's optional connection-version header behavior and the MCP version-negotiation boundary, without an SDK upgrade or modern-protocol migration.

The final native ping matrix has 20 cases (10 per mode); all previous 18 remain. A fifth Rust control test verifies explicit header preservation without duplication; the existing tests verify absence when the context supplied none. N02 code/dependencies and all prior acceptance suites are unchanged. Earlier preparation and CI receipts remain tied to their original candidates; only the final corrected head is eligible for N01 approval.

Reference: https://github.com/modelcontextprotocol/typescript-sdk/blob/main/packages/client/src/client/streamableHttp.ts and https://modelcontextprotocol.io/specification/2025-11-25/basic/transports#protocol-version-header

#### Measured compatibility before/after result

The existing PR candidate reproduced the unsupported offered-version header failure in both native modes while both no-ping controls passed. After correction, **161 compiled Linux tests** passed with zero failures/errors/skips, alongside **20/20 gated-ping cases** and **13/13 bounded-state cases**. This includes negotiated fallback to `2025-03-26` and the corresponding readiness header. N02 source and Cargo dependencies remained unchanged. Final cross-platform CI must be tied to the follow-up commit, not the earlier candidate.
