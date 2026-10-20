# Historical audit records

These reports preserve discovery, baseline measurements and original engagement requirements.
They are repository evidence, **not current instructions or authorization**. Commands, merge/push,
identity-repair and all-PR directives in their archived text describe that past engagement only.
Use current user-approved scope and current source/CI receipts for new work. Original test counts
are never attributed to a later tree.

## Current dispositions (source reviewed 2026-10-20)

| Report | Present source/disposition |
| --- | --- |
| [State/routing](2026-10-12.md) | Registration ownership/rollback: `src/register.rs`; irrevocable route identity/revision: `src/route.rs`, `src/config/document.rs`; bounded state: `src/storage/bounded.rs`; framing: `src/bridge.rs`. Original sub-poll limitation superseded by effective-route revisions for cooperating writers, not arbitrary external replay detection. |
| [Import bounds](2026-10-13-import-boundaries.md) | `src/import.rs` retains1MiB cap and handle checks. Nonblocking opened-handle reuse now addresses substituted FIFO; stalled kernel/network filesystems remain outside guarantees. |
| [Command/URL bounds](2026-10-13-command-boundaries.md) | `src/main.rs::validated_command` and `src/config.rs::public_proxy_url`; historical identity correction is not authority for another rewrite. |
| [Sign-in](2026-10-13-sign-in-verification.md) | `src/bridge/verification.rs`, `src/http_route/initialization.rs` and initialization control/response modules. Raw validation is required before normalization; current implementation extends it to normal JSON initialization. |
| [CI follow-up](2026-10-13-ci-followup.md) | `.github/workflows/ci.yml` retains macOS30-minute jobs and pinned stripping preflight. Current initiating trigger is manual, superseding historical automatic-event examples. |
| [Callbacks](2026-10-15-callback-boundaries.md) | `src/auth/local_callback.rs` retains effective-port binding and whole-header deadlines; callback transaction in `src/auth/callback_transaction.rs`. |
| [Refresh contention](2026-10-15-refresh-contention.md) | `src/credentials/refresh_lock.rs`: bounded wait, cancellation, OS ownership; not single-flight. |
| [Credential coordination](2026-10-15-credential-coordination.md) | Shared refresh/manual transaction retained; first-registration coordination additionally belongs to `src/auth/registration.rs`. |
| [Ping/state](2026-10-16-ping-and-state-bounds.md) | Pre-init ping/session/version/bounds retained. H01 bot-identity demand was **rejected**, preserving authentic signed bot attribution; later import-scaling report records that decision. No rewrite follows from the old demand. |
| [Import scaling](2026-10-16-import-scaling.md) | `src/import/names.rs` transaction-local first-free cursor and differential/probe-count controls retained; historical timing ratios are not portable guarantees. |

## Superseded and retained proposals

- Refresh waiting, route transition revisions and bounded import reads are implemented by the source
  above; earlier deferred language is historical, not an open defect.
- Native fixture Python pin now lives in `tests/native/requirements.txt` with one pip update route;
  full transitive hash locking remains unclaimed. Safe HTTP phase/status diagnostics are separate
  from resolving an unknown live network cause.
- General fuzz/property infrastructure, OS credential-store migration, single-flight refresh,
  periodic scheduled tests and multi-connection callback redesign remain unadopted proposals.
  No scanner, schedule, dependency or permission expansion is implied.
- Any recommendation not mapped here remains historically scoped/unknown until current evidence
  supports it. A report checkbox or imperative alone is not completion evidence.

See [verification evidence](../verification-evidence.md) for current evidence-layer limitations.
