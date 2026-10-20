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
Current host MCP connection failure remains an external observation, not a repaired network cause.
A live selected-route observation requires separate authorization, never an alternate direct route,
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
Route revocation and existing consistency guarantees remain unchanged.

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
