# Sign-in verification recheck — 2026-10-13

## Mandatory review authority

Read and obey the owner's actual private Rules and hooks before doing any work. Follow the complete Spec Kit chain prescribed there, including research, specification, plan, tasks, implementation, verification and integration receipts in the locations required by those Rules. Private Rules, AGENTS/CLAUDE files and `.specify` are not published in this checkout; their execution is not claimed by this audit.

This implemented correction is in [PR #7](https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/7). The auditing assistant must not merge or close any PR or issue. The owner's reviewing agent must review and dispose of every PR according to the Rules: merge eligible verified changes, or implement and verify a corrected equivalent before closing a superseded PR with traceable evidence. Fix newly reproduced defects in the same session rather than leaving an unexplained remainder. Do not declare DONE merely because a PR exists or was closed.

## Baseline and previous findings

Baseline: `d1af5025aac712a10463b65140f0f895e132397f`, version `2.1.1`, tree `e999f925ba7bc31607551c1fe369282d157df21b`.

PR #5 is already merged. Its bounded import, nonregular-input rejection, argument validation and explicit public-port fixes are not reopened. The exact baseline Linux artifact from [CI 38437364338](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/runs/38437364338) was downloaded, its outer and inner checksums verified, and its `release.json` source provenance checked.

Baseline executable SHA256: `606f65948083a5cb961dc77eaf6da3f7d06fde5943286527e80036235f3345af`.

A fresh native CLI regression pass completed 272 assertions in 119 bounded child processes with zero failures. It covered invalid invocations with no startup side effects, blocked mode, direct mode without credentials, default/nondefault ports including IPv6, sanitized URL round trips, atomic imports and duplicate handling, oversized/directory/FIFO inputs, malformed registration JSON, foreign-root ownership, idempotence, linked destination refusal and credential-safe logs. This is evidence about those exercised contracts, not a proof that every possible behavior is correct.

## V01 — false successful verification of an invalid initialize response

**Impact:** `sign-in` can print `Sign-in complete` and exit 0 even though the server did not provide a valid initialization response for this client. A stored OAuth token alone does not establish a working MCP session.

**Root cause:** the baseline code in `src/bridge.rs` only checks whether `result.serverInfo` is present. It does not validate the response ID, the `InitializeResult` schema, or whether the negotiated protocol version is supported by this handshake. In particular, JSON null still counts as a present value.

**Actual native reproduction:** a disposable loopback CONNECT/TLS fixture served synthetic OAuth metadata, registration, token exchange and MCP initialization to the real executable. The fixture never forwarded traffic to Upwork. Test trust overrides were confined to the child environment and temporary certificate files; only synthetic account state was used. The browser callback was submitted locally using the authorization state read in memory, never printed in evidence.

The 30-case native matrix included 17 malformed responses incorrectly accepted as success: wrong numeric/string response IDs; unknown, missing, null, empty, numeric or handshakeless protocol versions; missing, null, array or malformed capabilities; null, numeric or incomplete server information; invalid server name types; and invalid instruction types. Eight positive/compatibility controls succeeded. Four additional malformed/error cases correctly returned failure but exposed V02. The remaining negative control was a failure of readiness publication itself.

**Implemented repair:** `src/bridge/verification.rs` first checks the JSON-RPC response envelope and ID, then deserializes the actual pinned SDK `InitializeResult`. It accepts only members of `ProtocolVersion::KNOWN_VERSIONS` for which `has_initialize()` is true. The request uses `LATEST_WITH_INITIALIZE`, not `LATEST`, because the pinned SDK also knows a later lifecycle with no initialize handshake. Numeric request IDs accept the SDK-compatible numeric-string response representation; unrelated IDs remain rejected. Optional valid metadata is not rewritten or logged.

## V02 — readiness published before successful validation

**Impact:** the connector sends `notifications/initialized` after an explicit initialization error or malformed result, contrary to the successful-initialization ordering contract.

**Root cause:** the baseline sends readiness before checking even its weak `serverInfo` condition. This means a failure exit does not imply that readiness was withheld.

**Actual native reproduction:** an explicit JSON-RPC error, missing serverInfo, null result and empty result all returned nonzero exit codes but still produced exactly one readiness notification. V01's malformed responses likewise sent readiness.

**Implemented repair:** only a validated response reaches readiness publication. Validation failures send zero readiness notifications and return safe fixed errors. If the readiness POST itself fails, sign-in remains unsuccessful; it is not replayed. The existing persisted-session check still runs before the final success message.

## Production boundaries retained

Only sign-in's verification block delegates to the new private helper. Normal stdio relay, heartbeat timeout handling, selected-route behavior, blocked mode, OAuth exchange and credential persistence are not redesigned. The existing route-change guard, 30-second verification deadline and bounded cleanup remain. Dependencies, lockfile, release versions and CI/security gates are unchanged.

The matrix's immediate SSE success response preceded by a ping is only a positive compatibility control: it does not demonstrate handling a server that waits for a ping reply before returning initialization. No general support for that scenario, a new protocol lifecycle, hostile concurrent filesystem mutation, or live Upwork account operations is claimed.

## Regression suite and CI acceptance

Fifteen Rust tests are added inside the private verification module. Eight test groups cover IDs, envelopes, required fields, field types, supported lifecycle versions, optional metadata and safe errors. Seven use real loopback HTTP/SSE transport to verify exact readiness counts, explicit initialization errors, JSON/SSE positive cases, and failed readiness publication without replay. All fixtures have bounded deadlines and reap their owned server tasks.

The existing four-platform workflow discovers the library tests automatically. Require formatting, strict Clippy, every nextest suite, all native packaging/launcher/cleanup checks, dependency audit, CodeQL and dependency review on the final reviewed content. Preserve failed-run evidence; do not weaken checks or retry assertions until green. The first PR run stopped at two rustfmt layout differences before compilation; this same-session follow-up corrects them. A formatting failure is not a successful test run.

Final hosted results and native before/after receipts belong in the PR conversation, tied to the final head and content tree. GitHub PR CI may build a synthetic merge SHA: verify its tree equals the reviewed head rather than falsely labeling that merge SHA as an integrated main commit. Run the same native 30-case contract matrix and CLI regression suite on the checksum-verified final Linux artifact. A final test receipt must distinguish local native execution from hosted Rust compilation/tests and must not borrow earlier-head success.

Dependabot already covers Cargo and GitHub Actions daily. No duplicate updater or forced dependency migration is justified by V01/V02. The prior dependency-review prerequisite is not automatically treated as a new failure: this PR's first dependency review passed.

## Commit identity and integration gate

All historical and future published author and committer identities must use `Kiaro.Sama.Dev@gmail.com` as required by the owner. The connector's commit action has no identity arguments and created this PR's commits with the account's noreply email. This is explicitly not compliant and remains an integration gate, not a hidden exception.

The reviewing agent must inventory raw `%ae` and `%ce` values, retain a verified private recovery bundle and old-to-new mapping, coordinate affected refs under the Rules, repair nonconforming owner history, and verify unchanged content trees/topology and intended identity changes before a lease-protected push. Preserve provenance and handle signed commits/tags under the Rules; never silently discard someone else's work. A mailmap or coauthor trailer does not repair raw Git identities. Rerun final-head checks after any history repair. The assistant has not changed main or global account settings.

Do not make another competing implementation or unnecessary branch for these defects. Use this PR's existing branch unless the Rules require otherwise. Resolve all findings and all PR dispositions in the reviewing session, verify integrated main and final artifacts, and produce a final evidence-backed DONE receipt. If a real external permission or live-account requirement blocks completion, name it accurately rather than manufacturing a passing result.

## Primary references

- [MCP lifecycle: initialization, version negotiation and initialized ordering](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
- [Pinned rmcp 3.5.1 model: protocol versions, InitializeResult and response ID compatibility](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/model.rs)
- [Repository CI workflow](https://github.com/KiaroSama/Upwork-Agent-Plugin/blob/d1af5025aac712a10463b65140f0f895e132397f/.github/workflows/ci.yml)
- [Repository Dependabot configuration](https://github.com/KiaroSama/Upwork-Agent-Plugin/blob/d1af5025aac712a10463b65140f0f895e132397f/.github/dependabot.yml)

Continue researching official upstream documentation and relevant GitHub implementations when making additional changes, and record research in the locations mandated by the private Rules. Optional improvements must be evaluated separately from confirmed defects; do not turn this narrowly reproduced correction into an unbounded redesign.
