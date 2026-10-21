# Native acceptance parity ledger

Source: [`e844c50`](https://github.com/KiaroSama/Upwork-Agent-Plugin/tree/e844c50d4ee27dfe2bdee860190dd667f3f951a2/tests/native), Python3.13 and cryptography50.0.2. Source baseline hosted run39508492592 passed. The port's final CI must independently pass; baseline evidence is not target evidence.

| Source module | Rust responsibility | Preserved acceptance |
| --- | --- | --- |
| run.py | controller.rs | Archive SHA256, no archive links, single executable <40MiB, fresh contained output, immutable dev/inode/size/mtime,13 sequential suite processes,180s group kill/reap, sanitized receipts and final executable cleanup |
| probe_oracle_test.py | oracle.rs / controller lifecycle | Typed JSON ID/error, envelope exclusion, notifications, malformed/UTF8/duplicate output, causal rejection, crash/timeout and reader lifecycle; immutable digest reuse |
| cli_probe.py | cli.rs | 24 invalid commands/no side effects;48 URL settings/port/redaction/roundtrip checks; atomic duplicate/invalid/oversize/directory/FIFO import; registration malformed/foreign/idempotent/link preservation; credential-free logs;272 total assertions |
| protocol_probe.py | fixture.rs / initialization.rs / protocol.rs |30 initialization variants,8 positive contracts, readiness counts including failed initialized notification, callback/exit/completion agreement; loopback CONNECT allowlist/OAuth metadata/token/callback fixtures |
| extended_protocol_probe.py | initialization.rs / protocol.rs |20 raw/mixed/version/UTF8/framing/first-invalid/oversize variants;4 positive controls; readiness and exit agreement |
| terminal_sse_probe.py | child.rs / initialization.rs / protocol.rs |8 variants each manual/bridge; first terminal error/result wins; split/open/closed/string-ID variants; no private marker or malformed/duplicate output; prepared credentials via real sign-in |
| credential_coordination_probe.py | coordination.rs |3 real-process orders: manual waiting refresh,2 bridges reload rotation, refresh waiting manual; explicit gates, saved session and exchange counts |
| initialization_ping_probe.py | initialization.rs / protocol.rs |10 variants each manual/bridge; numeric/string/numeric-string IDs, multiple/log/no-ping/error/reply failure/overflow/fallback;16 reply cap, session/auth/version/readiness headers |
| state_reader_probe.py | state.rs |21 cases:4 valid dispatch controls; FIFO/directory/oversize/invalidUTF8/ignoredUTF8/dangling; regular state rejection classes and bounded process; oversized registration bytes unchanged |
| import_scale_probe.py | state.rs |8192 colliding labels, exact names/labels/count/anchor route/source bytes; exact output and duplicate reimport unchanged; wall/childCPU diagnostic only |
| route_revision_probe.py | coordination.rs |Observed owned SIGSTOP barrier, CLI block/restore, queued ping must never leave old route; inactive edit remains positive; resume/kill/reap only owned process |
| http_media_probe.py | initialization.rs / protocol.rs |14 media/body variants each mode,2MiB JSON positive versus16MiB overflow; exact/case/parameters/duplicate/missing/lookalike; causal typed rejection, one initialization |
| ordinary_http_probe.py | ordinary.rs |5 ordinary POST cases, actual listening GET ping/control, exact tools result and typed JSON ID, one init/ready/post, malformed media/chunked overflow/truncation diagnostic, no trailing duplicate terminal or secret stderr |

## Compatibility and cutover

- JSON numbers are not booleans; integer and string request IDs stay distinct. UTF-8 failures cannot become valid protocol or unrelated-negative success.
- Hashes bind executable bytes once per invocation and are reused in all receipts. Monotonic deadlines replace Python monotonic; no relaxed timeout or retry.
- OpenSSL creates disposable RSA2048 fixture CA/leaf with hostname SAN/server EKU; Rust TLS terminates CONNECT locally. Only owned child environments trust that CA; no system trust or live account requests.
- Observing an active child uses `try_wait`, not Tokio `wait`: the latter closes its stdin before awaiting, unlike Python `poll`. Waiting for final exit still explicitly closes/reaps it.
- Linux observed `/proc` stopped state replaces waitpid WUNTRACED, preserving an actual process-stop barrier rather than elapsed-time readiness.
- Receipt comparison is structural/semantic, not byte-exact for elapsed/CPU durations, random paths, certificate serials or timestamps. All assertion outcomes, counts, message types, route/state preservation and exits remain gating.
- Historical baseline-only diagnosis flags are not current acceptance modes. Source remains retrievable at the immutable commit for rollback; no old interpreter is required by current CI. Production behavior/version/release unchanged.
- Migration proof is synthetic packaged-executable replay, not production shadow traffic/canary or live-provider certification. Current source assertion tables and JSON-RPC contracts are independent expected behavior; source oracle regeneration is not claimed from static translation.
