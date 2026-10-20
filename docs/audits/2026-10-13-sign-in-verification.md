# Sign-in verification recheck — 2026-10-13

> Historical engagement record, not current instructions or authorization. Directives below describe the original engagement only, not current permission to execute, publish, merge or rewrite history. Receipts are baseline-specific. See [current dispositions](README.md).

## Archived engagement requirements and evidence

## Mandatory review authority

The original engagement required the owner's actual private Rules/hooks and their complete Spec Kit chain, including research and verification/integration receipts. Those private materials were not published in this checkout, and the audit did not claim their execution. This archived requirement does not direct or authorize work in a later session.

This correction was proposed in [PR #7](https://github.com/KiaroSama/Upwork-Agent-Plugin/pull/7). The original auditor had no merge/close authority. The original reviewer was responsible for verified disposition of its then-open PRs and reproduced defects; a PR's existence or closure alone did not satisfy that historical completion criterion. No present action is authorized here.

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

Only sign-in's verification block delegates to the new private helper. Normal stdio relay, heartbeat timeout handling, selected-route behavior, blocked mode, OAuth exchange and credential persistence are not redesigned. The existing route-change guard, 30-second verification deadline and bounded cleanup remain. The original correction left dependencies, lockfile, release versions and CI/security gates unchanged. The reviewing follow-up reuses the already-locked `sse-stream` 0.2.6 parser as a direct dependency for the manual verification boundary; package versions and normal relay semantics remain unchanged.

The matrix's immediate SSE success response preceded by a ping is only a positive compatibility control: it does not demonstrate handling a server that waits for a ping reply before returning initialization. No general support for that scenario, a new protocol lifecycle, hostile concurrent filesystem mutation, or live Upwork account operations is claimed.

## Reviewing-agent raw-envelope correction — 2026-10-14

Independent source review found that the pinned SDK's untagged message decoder ignores an `error` field when the same raw object also has a valid `result`. Serializing that typed response before validation loses evidence of the conflict. A raw-Value unit test alone does not exercise this production path.

Manual sign-in now enables a narrow initialize-POST decoder on the existing routed client. JSON and SSE envelopes are checked before SDK normalization. Mixed `result`/`error`/`method` forms fail with fixed safe errors; valid logging and ping events remain available to the SDK. SSE uses the existing locked parser with a pre-parse, chunk-aware per-event byte limit; JSON body reads are bounded. Other relay requests and readiness POSTs remain SDK-delegated. Production-seam wire regressions cover conflicting JSON/SSE responses with zero readiness.

The three original owner commits were corrected to the required raw public author/committer identity with verified private recovery and an exact old-ref lease. Their trees, dates, messages and parent topology are preserved; no signatures were removed. Subsequent source correction requires its own final-head CI and artifact receipt; older-head success is not approval of that correction.

## Regression suite and CI acceptance

Fifteen Rust tests are added inside the private verification module. Eight test groups cover IDs, envelopes, required fields, field types, supported lifecycle versions, optional metadata and safe errors. Seven use real loopback HTTP/SSE transport to verify exact readiness counts, explicit initialization errors, JSON/SSE positive cases, and failed readiness publication without replay. All fixtures have bounded deadlines and reap their owned server tasks.

The four-platform workflow discovered the library tests automatically. The original final-content gate included formatting, strict Clippy, every Nextest suite, native packaging/launcher/cleanup, dependency audit, CodeQL and dependency review. Failed evidence was retained rather than weakened or retried into apparent success. The first PR run stopped at two rustfmt differences before compilation; this follow-up corrected them, without counting the formatting failure as a test pass.

Original hosted/native before-and-after receipts were tied to the final head/tree. A synthetic GitHub test-merge SHA required tree comparison before attribution; it was not actual main integration. The engagement required the 30-case native matrix and CLI suite on its checksum-verified final Linux artifact, with local native and hosted compilation evidence kept distinct. These historical acceptance criteria authorize no present local compilation or duplicate test run.

Dependabot already covers Cargo and GitHub Actions daily. No duplicate updater or forced dependency migration is justified by V01/V02. The prior dependency-review prerequisite is not automatically treated as a new failure: this PR's first dependency review passed.

## Commit identity and integration gate

The original engagement required `Kiaro.Sama.Dev@gmail.com` for owner-attributed raw author/committer fields. The connector's commit action had no identity arguments and created this PR's commits with the account's noreply email. Those owner-attributed identities were noncompliant; the reviewing agent repaired the three commits before integration. The repair did not waive that engagement's final-head verification gate or imply that legitimate bot attribution needed rewriting.

That engagement's identity repair required raw `%ae`/`%ce` inspection, a verified private recovery bundle/mapping, coordinated affected refs, unchanged trees/topology and intended owner-identity verification before a lease-protected push. Provenance, other contributors and signatures were preservation constraints; mailmaps/trailers did not repair raw identities. Final-head checks followed the repair. The auditor did not change main or global account settings, and this record authorizes no later rewrite.

The original engagement required reuse of its existing implementation branch unless governing Rules required otherwise, disposition of its findings and then-open PRs, and separate integrated-main/artifact evidence. External permission or live-account prerequisites were explicit blockers rather than fabricated passes. This records past acceptance criteria, not current branch, account or publication authorization.

## Primary references

- [MCP lifecycle: initialization, version negotiation and initialized ordering](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
- [Pinned rmcp 3.5.1 model: protocol versions, InitializeResult and response ID compatibility](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/model.rs)
- [Repository CI workflow](https://github.com/KiaroSama/Upwork-Agent-Plugin/blob/d1af5025aac712a10463b65140f0f895e132397f/.github/workflows/ci.yml)
- [Repository Dependabot configuration](https://github.com/KiaroSama/Upwork-Agent-Plugin/blob/d1af5025aac712a10463b65140f0f895e132397f/.github/dependabot.yml)

The original engagement required fresh official/upstream research for additional changes, recorded under the private Rules. Optional improvements were evaluated separately from confirmed defects; the narrowly reproduced correction did not authorize an unbounded redesign.
