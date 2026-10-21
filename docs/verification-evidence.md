# Verification evidence and safety boundaries

## Skill evaluation

The nine `evals.json` entries are authored prompts, not native Claude plugin-eval cases or executed
model assessments. `tests/skill_traces.rs` runs deterministic synthetic action traces and mutation
controls: both approvals, unchanged preview identity, fresh account read, unchanged unrelated fields,
read-back, invalidated previews, uncertain outcomes and offline stopping. It verifies the trace
validator, not that a model follows the instructions. Cargo discovers this suite and the existing
manual CI runs it on four platforms. No account tools, model provider or billable evaluation is used.
A real model evaluation still needs selected provider/model, request budget and explicit authorization.

## Native client proof

Existing client installation instructions are not universal runtime certification. The historical
Claude install report lacks a complete public product/version/OS/source receipt. Other client rows
are structural/documentation-only. No missing details are reconstructed from a current installation.

Before an additional client run, select one actual product/surface/version/OS and authorize an
isolated temporary profile. Record source commit and package SHA256, exact manifest/resource paths,
loaded invocation keys for both skills, permission policy, normal offline drafting, missing-MCP
behavior and refusal to publish without both approvals. Preserve existing user plugins/settings.
Do not install a client, enable account tools or change permissions merely to complete the matrix.
No additional client has been selected or runtime-tested by this change.

## SDK recovery versus ambiguous outcomes

Pinned RMCP 3.5.1 can reinitialize after an explicit attached-session404 and retry the rejected
ordinary POST once. Its AuthClient can refresh after a challenge401 and retry when the token changes.
These are explicit-rejection recovery paths, not a blanket exactly-once promise. Control replies and
EOF, timeout or connection-reset outcomes must not trigger a new tool POST. Background GET resumption
is not a business-request replay. For uncertain writes, read current state before considering another
attempt; never automatically retry a write whose remote outcome is unknown.

## Interrupted HTTP investigation

Safe diagnostics distinguish initial request, numeric response status and late stream-body failure.
They do not establish which upstream provider, selected proxy or TLS peer closed a historical exchange.
Reproduce close-before-headers, truncated body, graceful SSE end and a valid control separately, with
request counters and bounded child ownership. Matching error labels alone do not prove equal causes.
Repeated installed GET body `UnexpectedEof` at roughly60-second intervals can coexist with a
successful subsequent metadata request: the SDK already owns GET resumption. Only an EOF at a
complete SSE event boundary is classified as a recoverable warning, retaining the error for the SDK,
its cursor and the same selected route. Partial-event and POST errors retain ERROR severity. The
actual HTTP stream status is recorded separately from any status attached to the typed error. A
successful reopened GET is logged, not inferred from an attempted reconnect. Actual TLS fixtures
close without `close_notify`, check complete versus partial framing, resume with `Last-Event-ID`,
deliver another event and complete a later POST with exact request counters. These tests establish
the local recovery/diagnostic contract, not which external peer omits TLS closure or uninterrupted
remote availability. The configured native metadata/help call succeeded during diagnosis.
A live selected-route observation requires authorization, never an alternate direct route,
account write, automatic login, private packet dump or timeout increase.

## Atomic-write orphan policy

Atomic state staging stays beside its destination for same-filesystem rename. Normal errors remove
owned scratch; abrupt termination can leave `<UUID>.tmp`. Existing UUID names alone do not prove
ownership, so automatic cleanup intentionally does not delete state-adjacent orphans.

| Object | Disposition |
| --- | --- |
| Active writer or held writer lock | Keep; do not interfere |
| Recent candidate | Keep; age is not ownership |
| Old unrecognized UUID file | Keep; never infer ownership from name |
| Symlink/reparse point or unexpected type | Refuse; never follow/delete target |
| Proven crashed-writer scratch with private recovery evidence | Explicit manual review only after all writers stop and original state is backed up |
| Existing `.cache/tmp/upwork-<UUID>.tmp` | Existing24-hour, regular-file, lock/maintenance checks apply |

Before manual recovery, identify the exact destination/writer, preserve original state, verify no
active writer, inspect only private local evidence and approve exact candidates. Do not move staging
to cache, recurse through account state or delete arbitrary backups. Crash-fixture execution and
additional-client/model behavior proof are distinct from this documented retained-orphan decision.

## Package provenance and promotion

The PowerShell source-build path captures Git HEAD/tree, compiler target/version and package version
before compilation and checks them again before recording the unsigned executable digest. It refuses
modified tracked or non-ignored untracked inputs. Packaging refuses missing Git/receipt, dirty inputs,
wrong source/tree/target/version/digest or preexisting output. This is local provenance, not a signed
builder attestation; ignored/generated inputs and change-then-restore races are not certified. The Unix
source-build entry point checks clean HEAD/tree before and after compilation and records the matching
unsigned digest, target and package version. Its owned process group is verified before Cargo starts;
wall/idle expiry and cancellation refuse publication and terminate that group.
For an approved Windows release, run `scripts/sign-package-stage.ps1` against a fresh extracted
unsigned stage inside this checkout's development work directory, with the reviewed unsigned
executable SHA256 and approved certificate thumbprint and DER SHA256. It checks unsigned metadata and
bytes before using the existing current-user key, verifies
the signer/native result, and atomically updates `release.json` with `unsigned_binary_sha256`, the final
`binary_sha256`, `windows-authenticode-sha256` transformation and certificate fingerprint. A failed
signing stage is disposable: preserve the original archive and extract again, never relabel or retry
uncertain transformed bytes. UTC safe lifecycle logs use `sign-stage_<timestamp>_<run-id>.log` in the
development work directory; no raw signature errors, keys or certificate-store contents are logged.
Repackage only after success; recalculate archive checksum, verify detached signatures and native bytes
again after downloading. This helper does not publish, install, configure trust or replace final
cryptographic/tamper checks. Its CI boundary-mocked policy test is not real signing evidence. Signed
bytes must never be claimed equal to unsigned bytes. A source archive without verified provenance is
not relabelled as a checkout.

Archives and checksums are staged under one owned pending directory. Live failure removes only that
invocation's promoted outputs. Promotion of several files is not crash-atomic: interruption may leave
an incomplete set of individually validated products. Preserve and verify them against the original
receipt, then explicitly resolve that exact set before retrying; never overwrite or broadly clean dist.
The skills-only ZIP has its own landing README and no native executable or launcher.

## Route-validation cost

The manual CI measures three samples of sequential checks, an idle guard and two concurrent guards
for both one-profile and8192-profile synthetic configurations. The diagnostic records wall time,
fallible process CPU time, snapshot-read attempts, OS/architecture and available parallelism. CPU
includes the isolated test process/runtime, not other Nextest processes; clock resolution may yield
zero for short samples. Snapshot counts are not kernel read calls or physical disk operations.
Instrumentation is compiled only into unit tests, not the released executable. No timing ratio is a
pass criterion, and these measurements do not attribute host-wide load or authorize an mtime cache.
Every check still reads fresh bounded regular-file bytes. Exact equality to the last fully validated
same-route document reuses its validity; any byte difference requires full UTF-8, schema, profile,
selection and revision validation. This private bounded in-memory document is shared by Route clones,
never logged or serialized; it is not an mtime cache and adds no freshness window. Parse counts are
recorded separately from read attempts. Route revocation and existing consistency guarantees remain
unchanged.

## Relay admission

The relay admits at most16 outstanding ordinary sends and16 control responses independently. Control
responses can reach the SDK's separate control queue while ordinary POSTs are held; ordinary messages
retain admission order and the SDK still permits only two ordinary HTTP requests at once. Input frames
remain bounded to1MiB. A next frame exceeding its category's budget terminates explicitly, without
silently dropping a message or replaying uncertain work. This replaces the previous16-total admission
backpressure contract. Route change and downstream EOF remain cancellation paths; a host that stops
reading stdout can still impose output backpressure.

## Bounded cleanup and verification receipts

Native cleanup inspects at most1000 entries plus one lookahead. `inspection_truncated=true` means the
report is prefix-only, not a complete inventory. Unknown/active/private data remains untouched.
PowerShell verification retains one unique JSON receipt per invocation under the development work
directory; stage and command digest identify a command without storing private arguments/environment.
Each invocation also retains a UTC `verification_<timestamp>_<run-id>.log` with fixed lifecycle/outcome
metadata only; compiler output remains in the captured console, never copied wholesale into this log.
No argv, environment, tokens or upstream error text is serialized. There is no automatic retention
purge; review development evidence explicitly. The manual workflow uploads the full receipt collection
and safe lifecycle logs. Nextest limits apply only to its CI profile;
serial libtest has the outer wrapper bounds, not per-case Nextest isolation.

Before Cargo starts, the verification driver establishes a Windows kill-on-close Job Object or a
verified Unix process group. Cleanup does not depend on Cargo's root PID remaining alive. A nominal
successful command with surviving owned descendants fails verification; wall/idle expiry terminates
the owned group/job. Windows compiler commands (`clippy`, `build`, `nextest`) have one narrow auxiliary
custody policy: a surviving `vctip.exe` must belong to the owned Job, reside in a non-reparse installed
Visual Studio/MSVC path under Program Files, and carry a valid Microsoft Corporation signature.
Only when every live member qualifies is the entire Job terminated and verified empty without changing
Cargo's original exit code; `compiler_auxiliaries_settled` records the count. This is explicit cleanup,
not natural exit. Unknown/impostor descendants still fail, including ordinary sleeper fixtures.
Receipts distinguish cleanup from detected survivors and timeout. On Unix the
receipt means no running group members and the direct child waited; orphan zombies belong to init,
and arbitrary descendants deliberately escaping their group are not certified. This is build-tool
lifetime ownership, not a hostile-code sandbox. Native CLI rejection checks require controlled exit1
without an owner timeout or invalid output capture; crash/signal/timeout controls cannot pass as
expected product rejection. Existing state-preservation assertions remain mandatory.
