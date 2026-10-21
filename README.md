# Upwork Agent Plugin

<div align="center">

[![CI](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml)
[![GPL v3](https://img.shields.io/badge/License-GPL_v3-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/Source-Rust-dea584)](#implementation)
[![Rust 1.99+](https://img.shields.io/badge/Build-Rust_1.99%2B-dea584)](#optional-source-build-developers-only)
[![Windows](https://img.shields.io/badge/Platform-Windows_11-0078d4)](#installation)
[![Linux](https://img.shields.io/badge/Platform-Linux-f5a623)](#installation)
[![macOS](https://img.shields.io/badge/Platform-macOS-8b5cf6)](#installation)
[![PowerShell 7](https://img.shields.io/badge/Launcher-PowerShell_7-5391fe)](#installation)
[![RMCP](https://img.shields.io/badge/RMCP-3.5.1-7c3aed)](https://docs.rs/rmcp/3.5.1)
[![Reqwest](https://img.shields.io/badge/Reqwest-0.13.5-0284c7)](https://docs.rs/reqwest/0.13.5)
[![MCP stdio](https://img.shields.io/badge/Local_transport-MCP_stdio-2563eb)](#desktop-entry)
[![HTTP proxy](https://img.shields.io/badge/Proxy-HTTP-0ea5e9)](#routing)
[![HTTPS proxy](https://img.shields.io/badge/Proxy-HTTPS-16a34a)](#routing)
[![SOCKS5 proxy](https://img.shields.io/badge/Proxy-SOCKS5_remote_DNS-0d9488)](#routing)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

Native Rust connector for **Upwork's hosted MCP**, with named fail-closed proxies, manual OAuth and
original multi-client project-selection guidance. No Node or `mcp-remote` runtime is invoked.
Upwork's server remains hosted; this independent project is not the official Upwork plugin, an offline
Upwork service or a self-hosted backend.

## Installation

There are two independent installs: the **native Rust MCP connector** and the **instruction plugin**.
Installing the plugin does not start the executable, register MCP or sign you into Upwork. The plugin
alone needs no Rust/Node dependencies. Never copy private profiles, OAuth state, logs or caches into a
plugin installation.

### 1. Download a ready-to-run build

Open [Releases](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest). Download **one** archive
for your machine and its matching `.sha256` file, not GitHub's source-code ZIP. No Rust, Cargo, Node,
Git or local compilation is needed.

The archive names below match source version **2.1.12**. Use them only after that version appears
in Releases; verify the matching checksum and pinned owner signature before running. Never rename
an older executable to match a new release.

| Machine | Download |
| --- | --- |
| Windows x64 | [upwork-agent-plugin-2.1.12-windows-x64.zip](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.1.12-windows-x64.zip) |
| Linux x64, glibc 2.35+ | [upwork-agent-plugin-2.1.12-linux-x64.tar.gz](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.1.12-linux-x64.tar.gz) |
| macOS Apple Silicon | [upwork-agent-plugin-2.1.12-macos-arm64.tar.gz](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.1.12-macos-arm64.tar.gz) |
| macOS Intel | [upwork-agent-plugin-2.1.12-macos-x64.tar.gz](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.1.12-macos-x64.tar.gz) |

Each archive contains the executable in `bin/`, its launchers, skills plugin and GPL license.
`release.json` records the version, platform and source commit; corresponding source is available at
the release tag. Extract the **whole archive** into a new private, writable folder and keep the layout.
Do not move only the executable or overwrite existing profiles, saved sessions, input files or logs.

Verify SHA256 before running. On Windows, from your downloads folder:

```bash
powershell.exe -NoProfile -Command "Get-FileHash ./upwork-agent-plugin-2.1.12-windows-x64.zip -Algorithm SHA256"
```

Compare the displayed hash with the matching `.sha256` file. On Linux:

```bash
sha256sum -c upwork-agent-plugin-2.1.12-linux-x64.tar.gz.sha256
```

On macOS Apple Silicon (use the x64 filename on Intel):

```bash
shasum -a 256 -c upwork-agent-plugin-2.1.12-macos-arm64.tar.gz.sha256
```

Alternatively, download the platform artifact from a successful exact-commit
[CI run](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml) while retained
(14 days; GitHub sign-in may be required). Extract the outer artifact ZIP first, then the platform
archive. Releases provide durable public downloads.

Public Windows executables carry KiaroSama self-signed Authenticode signatures; all release archives
have detached signatures. This is not public CA trust or Apple notarization. Verify the pinned signer
using the release `SIGNATURES.md`, then follow your OS security policy rather than disabling protections. Windows ARM, Linux ARM, Alpine/musl and other CPU
targets are not covered by these bundles.

Owner certificate SHA256: `c99fcd4e2670b15dde60e65d1f846e9c244234b652b2832eefa60845716dd2a7`.
Release signatures do not install trust roots, carry an Apple identity or include a third-party timestamp.

### 2. Run on Windows

Open the extracted folder. Protect private state before saving profiles or signing in:

```bash
powershell.exe -NoProfile -File ./scripts/protect-state.ps1 -ProjectRoot .
```

PowerShell 7 is needed only for the optional launcher/developer scripts:

```bash
pwsh -NoProfile -File ./run.ps1
```

Launchers pass project-contained cache paths to native startup without creating directories first.
The native connector validates the complete command and rejects linked or invalid cache/temp paths
before creating runtime directories. Launcher diagnostics use timestamped severity-tagged stderr;
native runtime logs remain under `logs/`. No alternate cache or route is selected after rejection.

HTTP failures record fixed error classes, typed request/connect/DNS/body flags, numeric status
(`0` means unavailable), and a bounded I/O error-kind summary. URLs, tokens and source-error text
are never included. An `UnexpectedEof` at a complete GET event boundary is a recoverable warning:
the SDK retains its event cursor and resumes GET without replaying POSTs. A successful GET reopening
is logged separately; it is not assumed from a retry attempt. Incomplete events, POST failures and
failures before response headers remain errors. `http_status` records the received stream status;
the error's own `status=0` still means unavailable. This does not disable TLS checks or repair a
remote peer that closes without `close_notify`; no proxy fallback is introduced.

The native menu needs no PowerShell or compiler:

```bash
./bin/upwork-connector.exe menu
```

### 3. Run on macOS or Linux

Create a private folder and extract the archive (Linux example; choose your platform filename):

```bash
mkdir -m 700 upwork-agent-plugin
```

```bash
tar -xzf upwork-agent-plugin-2.1.12-linux-x64.tar.gz -C upwork-agent-plugin
```

Unix archives preserve executable permissions. The optional launcher needs `sh`, not PowerShell:

```bash
sh ./upwork-agent-plugin/run.sh
```

From the extracted directory, the native executable also runs directly:

```bash
./bin/upwork-connector status
```

Use `./bin/upwork-connector` in place of the Windows `.exe` in subsequent commands. The same stdio
executable works with MCP hosts on Linux; host application availability is separate from connector support.

### 4. Connect and verify

Downloaded binaries create no Cargo build cache. See [Cache and state paths](#cache-and-state-paths)
for runtime storage. Setup requires access to your explicitly selected route and an authorized Upwork account.

1. **Local proxy:** choose **8 — Local proxy settings** to match your existing proxy (default
   `socks5h://127.0.0.1:1595`), then **9 — Select local proxy**. This does not install a proxy service.
2. **Saved remote proxy:** choose **2 — Import proxy list from file**, enter its UTF-8 path, then
   **1 — Select proxy**. Initial mode is blocked; importing does not enable networking.
3. Choose **6 — Sign in** yourself. Copy the complete authorization URL into your browser on this
   computer. Keep the terminal open until saved-session/MCP-initialization confirmation.
4. For Claude Code, choose **7 — Register**, reopen the project and approve/reconnect `Upwork-Agent-mcp`.
   For Desktop, use the entry below. Registration preserves other servers and refuses an unrelated
   existing `Upwork-Agent-mcp` entry.

`0` returns from submenus, `exit` quits, and Enter in the main menu opens selection.
**4 — Direct** deliberately bypasses proxies; **5 — Block** disables connector networking.
Sign-in, MCP registration and instruction-plugin installation are separate operations.

## Desktop entry

After extracting the build, generate the exact configuration for your executable and folder:

```bash
./bin/upwork-connector.exe desktop-config
```

Merge the printed entry into `mcpServers` in `claude_desktop_config.json`, preserving other servers.
Or use this complete example, replacing `<absolute-project-root>` with your installation folder:

```json
{
  "mcpServers": {
    "Upwork-Agent-mcp": {
      "command": "<absolute-project-root>/bin/upwork-connector.exe",
      "args": ["bridge"],
      "env": {
        "TEMP": "<absolute-project-root>/.cache/tmp",
        "TMP": "<absolute-project-root>/.cache/tmp",
        "TMPDIR": "<absolute-project-root>/.cache/tmp",
        "XDG_CACHE_HOME": "<absolute-project-root>/.cache"
      }
    }
  }
}
```

Windows JSON paths can use `/` or escaped `\\`. On Unix remove `.exe` from `command`.
Save the file, completely quit Desktop and reopen it. **Claude starts and owns the native stdio process;
no launcher needs to remain open.** The executable resolves the installation root from its `bin/`
location; caller CWD and spaces do not change state storage. Use the menu only for initial sign-in
or changing settings. Do not replace the command with `cargo run`, Node or a hosted HTTP entry.

An explicit `--root` override remains available for developer/nonstandard layouts; generated entries
retain it when needed. It binds state, not the OS temporary-directory environment.

Registration only edits Claude Code's project configuration. Its direct-Upwork denial does **not**
apply to separate Desktop or claude.ai connectors. Disable duplicate direct Upwork connectors in that
host yourself if routed-only access is required. Optional official Upwork skills can remain installed;
they are not bundled here.

## Routing

HTTP/HTTPS/SOCKS5/SOCKS5h. HTTP proxies forward HTTP and tunnel HTTPS with CONNECT; HTTPS proxies encrypt
the connection to the proxy. SOCKS5 is normalized to SOCKS5h for remote destination DNS. Every connector-owned
MCP, OAuth discovery, registration, token exchange and refresh request uses the selected route.
A failed proxy causes an error, **never direct/alternate fallback**. Environment/system proxies and
`NO_PROXY` cannot bypass the explicit route.

A route change invalidates the old connection when the watcher observes it (100 ms interval); new
requests also check selection. Valid label and inactive-profile edits do not change the effective route.
Observed invalidation permanently revokes the old connection and its clones, even if settings are restored.
Cancellation covers stalled protocol output and OAuth response bodies; SDK shutdown is separately bounded
to two seconds. Interrupted work or partial protocol output is not replayed automatically. Reconnect the MCP host.
An authorization-server mismatch fails closed without deleting the original session.
Every check reads fresh bounded configuration bytes; only byte-identical already validated state can
reuse its parse. Different bytes are fully validated, including inactive profiles and route revision.
No timestamp-only cache or new freshness delay is introduced.

Relay admission has separate16-ordinary and16-control-response budgets. Control replies can progress
while ordinary sends are held; ordinary order and two concurrent ordinary HTTP slots are retained.
A frame exceeding its category's budget fails explicitly instead of growing an unbounded queue.
Frames remain limited to1MiB; unknown remote write outcomes are never automatically replayed.

Proxy-side DNS prevents local destination lookups, but cannot guarantee a resolver in the proxy exit's
country. Configure upstream DNS/DoH in your local proxy application. Browser DNS/routing/timezone,
Claude model traffic and other MCP servers are outside this connector. The connector does not read the
device timezone or create missing timezone hints. Before forwarding, existing device hints in MCP
`params._meta` are normalized: `timezone`/`timeZone` become `UTC` (Greenwich, zero offset), and
`timezone_offset_minutes`/`utcOffset`/`utc_offset`/`offsetMinutes` become numeric `0`. The same rule
applies inside the optional `openai/userLocation` metadata object. Other fields, tool arguments,
business dates/meeting timezones and incoming server messages stay unchanged; arbitrary custom nested
metadata is not recursively rewritten. Windows and the sign-in browser timezone are not changed.
This does not hide location inferred from IP, explicit geographic hints or user-provided text;
UTC log/expiry timestamps alone are not timezone masking.

<a id="route-transition-revisions-and-build-deadlines"></a>
### Route transition revisions

Current connector writers atomically store an internal `_route_revision` only when the canonical effective route changes. A completed route cycle revokes old connections even between watcher observations; inactive-profile, label, ordering and equivalent-URL edits do not disconnect them. Legacy files remain readable without read-time rewriting. This marker does not authenticate external edits or detect every unobserved historical-document replay. Reconnect after an effective route change. Build deadlines are documented under [source builds](#optional-source-build-developers-only).

## Local proxy settings

**8 — Local proxy settings** edits the endpoint; default is `socks5h://127.0.0.1:1595`.
Change scheme, host and explicit port to match your existing proxy. **9 — Select local proxy** applies
it; reconnect MCP afterward. No proxy service is installed or started. Editing settings alone does not
switch routes or rewrite imported profiles, including their ports.

Agents read the sanitized setting with `local-settings`, update it using `local-settings set` with
`{"url":"http://127.0.0.1:1595"}` on stdin, and select it using `local-proxy`. Credentials belong only
in protected stdin/config, never command arguments. This is not the OAuth callback port or Upwork's
HTTPS destination port 443; this connector does not provide a local HTTP MCP listener.

## Cache and state paths

The executable derives the root from its installed location. Generated Desktop/project entries direct
`TEMP`, `TMP` and `TMPDIR` to `<installation>/.cache/tmp`, and `XDG_CACHE_HOME` to `<installation>/.cache`.
The executable creates these private directories; launchers apply the same per-process environment.
No persistent HTTP response cache exists.

Profiles/settings stay in `config/`, account state in `.state/` and logs in `logs/` under the installation.
Atomic-write scratch files stay beside their destination. Downloaded runtime bundles contain no build cache.
Build/verification scripts confine temporary files, Cargo downloads and compiler output to this project;
CI also contains its downloaded toolchain and installer/advisory caches inside its checkout.
Existing shared tools are not moved or deleted.

State writers use persistent OS-lock files: file existence alone does not mean a writer is active.
Locks are released on guard destruction or process termination; do not delete coordination files while
processes run. When upgrading from the earlier sentinel-lock implementation, stop every old connector,
menu and bridge for this installation before replacing the executable. Preserve sessions and profiles.
Project registration refuses foreign roots, malformed configuration and linked destinations; handled
publication failures restore original bytes only when no intervening settings edit occurred. This is
not a crash-atomic transaction across both configuration files. See the [audit notes](docs/audits/2026-10-12.md).

These paths do not relocate Claude Desktop's own application caches, other MCP servers' files or the
separate browser's data. Preserve profiles, saved sessions, input files and operational logs during cleanup.

## Proxy import and management

One record per UTF-8 line, with optional BOM, blank lines and `#` comments; example values only:

```text
socks5://proxy.example.com:12004:example-user:example-password:UK 5
http://proxy.example.com:8080:example-user:example-password:UK 1
```

The final field is the display label, including spaces. Standard proxy URLs also work.
Colon-delimited credentials cannot contain literal colons; use percent-encoded URL userinfo instead.
Imports append validated profiles and skip identical normalized URLs, including credentials.
One bad line rejects the complete import without partial writes; errors show only the line number.
Import accepts only regular files (including symlinks to regular files), up to 1 MiB. The reader also
enforces that bound if a file grows after its metadata is checked. Pipes, devices and directories are rejected.
Import never changes the active route or modifies the input file. Public `status` and local-settings URLs
remove credentials but retain an explicit port, including HTTP 80 and HTTPS 443, so they remain valid inputs.

```bash
./bin/upwork-connector.exe import /absolute/path/to/my-private-proxies.txt
```

```bash
./bin/upwork-connector.exe status
```

```bash
./bin/upwork-connector.exe select UK-1
```

`select` uses the internal `name` returned by `status`, not a spaced display label. Other commands:
`add` (JSON stdin), `direct`, `block`, `sign-in`, `bridge`, `register`, `desktop-config`, `preview`,
`local-settings [set]` and `local-proxy`. Agents with local shell access use these same commands;
no extra management MCP tools are introduced. Unsupported extra arguments fail before startup cleanup or
configuration changes; put `--root PATH` before the command. An unsupported `direct --help` never enables
Direct mode. Reconnect after route changes; never put credentials in arguments.

### Bulk import scaling

Colliding display labels retain the same first-free identifiers, including holes and nested names. A transaction-local cursor avoids repeatedly scanning occupied prefixes; duplicate URLs consume no names. The 1 MiB input limit, active route, labels and atomic publication contract are unchanged. Deterministic probe-count and differential tests gate this optimization; recorded runner timings are diagnostic, not portable speed guarantees.

## Manual OAuth

```bash
./bin/upwork-connector.exe sign-in
```

The official SDK handles protocol/OAuth state machines. The human copies the complete genuine URL into
an external browser on this computer; no automatic browser launch occurs. Keep the terminal open until
usable persisted credentials **and successful MCP initialization** are confirmed. A browser callback
page alone is not proof of completion. Manual verification checks the matching response ID, typed result
and supported initialize-based protocol before sending readiness. Conflicting JSON/SSE response envelopes
are rejected before SDK normalization; invalid responses cannot claim successful sign-in.

Legacy sessions are read without modifying their files; native refresh/sign-in writes separate Rust state.
Concurrent refresh and manual sign-in share one credential transaction lock. A healthy contender waits
up to 60 seconds; invalid storage still fails immediately. Both operations retain ownership through
exchange and persistence, so an older refresh cannot overwrite a completed manual sign-in. Waiting
refreshes reload rotated credentials; this is serialization, not single-flight deduplication or HTTP replay.
An authorization-server mismatch fails closed rather than deleting original credentials.
See [migration notes](docs/migration.md) for compatibility and rollback.

If your browser proxy blocks localhost, exempt localhost/127.0.0.1 in **browser settings only**;
connector-owned requests still use the selected route. Never share tokens, authorization links or callback
parameters in support logs. Saved credentials require private directory access.

## Project-selection guidance

The original [upwork-project-selection](skills/upwork-project-selection/SKILL.md) skill evaluates buyer
readiness, freelancer fit and contract value before spending time or Connects. It is advisory/read-only;
installation neither registers MCP nor authorizes messages, proposals or Connects spending.

The project root contains the shared `skills/` tree, root `plugin.json`, and Claude/Codex/Cursor
compatibility manifests. The Claude marketplace is `upwork-agent-plugin-marketplace`; the plugin is
`upwork-agent-connector`. Packages contain no saved profiles, account state or direct hosted MCP declaration.

### Install the Claude Code plugin

```bash
claude plugin marketplace add KiaroSama/Upwork-Agent-Plugin
```

```bash
claude plugin install upwork-agent-connector@upwork-agent-plugin-marketplace
```

```bash
claude plugin details upwork-agent-connector
```

Start a new session and invoke `/upwork-agent-connector:upwork-project-selection`.
For local session-only loading, use `claude --plugin-dir /path/to/extracted-plugin`.

### Install the Codex plugin

```bash
codex plugin marketplace add KiaroSama/Upwork-Agent-Plugin --ref main
```

Restart the ChatGPT desktop app, open **Plugins Directory**, choose **upwork-agent-plugin-marketplace**
and install **Upwork Account Workflows**. Invoke `$upwork-project-selection`. Surface/version availability
varies; this is not a universal `codex plugin install` command. Standalone copying also works.

### Install the Cursor skill

Download the [skills-only ZIP](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-selection-plugin-2.1.12.zip).
Copy `skills/upwork-project-selection/` to the workspace's `.agents/skills/upwork-project-selection/`.
Preserve existing files, keep only one discovery copy, reopen the workspace and invoke `/upwork-project-selection`.

### Install the Antigravity skill

Use the same original skill directory and workspace `.agents/skills/upwork-project-selection/` path.
Use a real copy, not a symlink, and inspect an existing destination before replacement. Reopen the workspace
and invoke `/upwork-project-selection`. No native Antigravity plugin-install command is claimed here.

### Installation boundaries

See [complete client instructions](docs/skills.md) for collision-safe Windows/Unix copying, manifest details
and verification limits. Historical Claude validation/install reports lack a complete public product/version/OS/source receipt;
Codex/Cursor/Antigravity checks are structural/current-documentation evidence, not native runtime proof.
Nine authored evaluation prompts (four selection, five profile) are not executed model evaluations or proof of live marketplace success or statistical effectiveness.
Official Upwork skills remain separate; no global installation, permission expansion or duplicate route is enabled automatically.

## Profile management

The new [upwork-profile-management](skills/upwork-profile-management/SKILL.md) skill adds a future workflow
for title, biography, skills, employment, education, language and availability changes. Invoke
`/upwork-agent-connector:upwork-profile-management` after installing/updating the plugin.
It reads current profile evidence, preserves untouched fields, presents the complete proposed diff and
requires explicit approval before creating a write preview, then a second approval before confirmation.
Installation alone never drafts or edits your profile. Unknown experience or metrics are never invented.
For an explicitly offline draft, paste the facts: no MCP call is needed, supplied information is labeled
potentially stale, and no current-account diff is invented. Later publication starts with fresh live
account/profile reads and still requires both approvals. See [evidence boundaries](docs/verification-evidence.md).
The hosted Upwork MCP already supplies these operations; the connector does not fabricate new APIs.

Project selection now includes [competition and client-history evidence](skills/upwork-project-selection/references/competition.md):
exact proposal counts only when exposed, otherwise ranges; posting activity separately from lifetime
client hires/spend; sampled work history is not a total. Unavailable metrics remain unknown.

## Safe maintenance

Every native run attempts conservative cleanup of flat `.cache/tmp/upwork-<UUID>.tmp` files at least
24 hours old. Unknown names, fresh files, links and locked files remain untouched. A nonblocking
maintenance lease prevents automatic cleanup while another cooperating connector is active.
No generic browser/host cache, account session, profile configuration, proxy input or operational log is deleted.

Inspect eligible stale owned temporary files:

```bash
./bin/upwork-connector.exe cleanup
```

Apply only that temporary-file cleanup after closing active connectors:

```bash
./bin/upwork-connector.exe cleanup --apply
```

For project build caches and confirmed legacy software remnants, use the separate optional PowerShell7
script. It defaults to a dry-run and refuses active connector/build/Node processes, linked paths or
protected content inside targets. It never deletes the current `bin/` executable, `.state/`, `config/`,
input files or `logs/`. Its allowlist covers `target/`, `.cargo-cache/`, `.ci-cache/`, `.ci-work/`,
legacy `.runtime/`, `node_modules/`, `.cache/npm/` and the specifically named previous native binary.
It is conservative: if an unrelated build process is active, cleanup also waits rather than guessing ownership.

```bash
pwsh -NoProfile -File ./scripts/cleanup.ps1
```

After reviewing the listed targets and closing relevant processes:

```bash
pwsh -NoProfile -File ./scripts/cleanup.ps1 -Apply
```

Maintenance logs use the same UTF-8 UTC format in `logs/`, with safe counts and no credential/file contents.
Cleanup does not run continuously while Claude is closed; it runs at native startup or on explicit request.

## Logging

Each run creates a UTF-8 log in `logs/`, such as `upwork-native_YYYY-MM-DD_HH-mm-ss_UTC.log` or
`upwork-bridge_YYYY-MM-DD_HH-mm-ss_UTC.log`; collisions receive a suffix. Entries use
`[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message`. INFO, WARNING and ERROR record fixed safe
startup, callback, failure and shutdown events. File initialization failure falls back to safe stderr.

Transport diagnostics record fixed categories such as `control_post_timeout`, `session_expired`,
`proxy_or_connect`, `credential_store` and `unexpected_http_response`, plus HTTP operation stage and elapsed
milliseconds where available. A control POST timeout includes authentication/refresh time, not just network
latency. A timed-out result for an observed server ping records a WARNING and keeps the same transport
open: the SDK releases that control slot, and the connector never replays the reply. Unmatched replies,
ordinary requests and other send failures still fail closed; route changes still stop the old connection.
These categories do not automatically retry or replay a request. Older generic logs cannot identify
the underlying cause retrospectively. Project registration migrates only an owned `upwork-local` entry to
`Upwork-Agent-mcp`; conflicting or foreign entries are refused before any publication.

No raw upstream errors, account payloads, credentials, tokens or authorization links are logged.
The complete login URL appears only in explicit human sign-in. No debug passthrough or automatic retention
exists; share only reviewed diagnostics. Preserve real operational logs during cleanup.

Windows setup protects config, credentials, cache and logs with owner/SYSTEM-only permissions.
After moving the project, restore those permissions before saving secrets:

```bash
powershell.exe -NoProfile -File ./scripts/protect-state.ps1 -ProjectRoot .
```

## Implementation

Rust 2024 modules own profiles, import, routing, credentials, OAuth, MCP relay, logging and menu.
The official [RMCP SDK](https://github.com/modelcontextprotocol/rust-sdk) supplies protocol/OAuth state
machines; [Reqwest](https://github.com/seanmonstar/reqwest) supplies explicitly configured HTTP/proxy clients.
Both OAuth redirect policies use routed clients; SDK defaults are never used for outbound requests.
The relay preserves JSON-RPC messages rather than enumerating/hardcoding Upwork tools. SDK debug tracing
is deliberately disabled because vendor events can expose OAuth secrets. During initialization, the first
terminal SSE result or error is preserved for both manual sign-in and the normal bridge. An error cannot
be replaced by a later success; no readiness notification is sent after failed manual verification.
Selection does not wait for stream EOF, buffer the entire stream or replay the request, and remains
inside the selected-route cancellation guard.

## Optional source build (developers only)

Ready-made downloads are the normal installation path. Contributors need Rust 1.99+, Cargo and a native linker.

```bash
pwsh -NoProfile -File ./build.ps1
```

On macOS/Linux:

```bash
sh ./build.sh
```

Source builds can consume several GB under `target/`; user installation never needs that cache.
The shell build requires monitor-mode process groups (it reuses installed Bash when `sh` lacks them),
Git and SHA256 tooling; it installs nothing. Both entry points use 900-second wall/600-second idle
release bounds and preserve the installed binary when compilation fails. Shell builds capture Cargo
output in `logs/upwork-build_<UTC>_<pid>.log`; inspect that file for progress/errors. Optional
`UPWORK_BUILD_WALL_SECONDS`/`UPWORK_BUILD_IDLE_SECONDS` may shorten, never increase, shell limits.
Clean Git inputs and matching executable receipts are required for verified release packaging.
Ordinary verification retains 900/180 seconds. Optimization, stripping and failure exit handling remain
enabled; silence alone is not proof of compiler progress. PowerShell verification requires installed
Bash on Unix and verifies its isolated process group before Cargo starts; Windows uses a kill-on-close
Job Object. Surviving owned descendants fail verification even after Cargo exits successfully.
Build/verification scripts keep generated paths inside the project. Preserve `bin/`, profiles, session,
input files and operational logs when clearing optional development artifacts; shared tools remain untouched.

### Initialization controls and state limits

During SSE initialization, the connector answers server pings with the original ID over the same
selected route before waiting for the terminal response. Control replies have a five-second deadline;
initialization has one 30-second deadline, a 16-ping budget and a 256-event budget. An offered protocol
version is not invented as an already-negotiated header. No initialize request is replayed.

Internal JSON state reads and writes have a shared 16 MiB byte limit, including serialized JSON's
final newline. Special files and oversized state are rejected without truncating existing data.
Regular-file links remain readable for profiles/settings/credentials; registration refuses leaf links.
The separate proxy-import input limit remains 1 MiB. Accumulated valid state may exceed 1 MiB.
Back up and explicitly reduce older state larger than 16 MiB before retrying; the connector does not
silently trim it. These checks do not guarantee interrupting a stuck kernel or network filesystem.

### HTTP response boundaries

Strict sign-in, ordinary POST replies and listening GET streams validate one complete, case-insensitive
`application/json` or `text/event-stream` media type with valid HTTP parameters. Mixed-case and quoted
parameters work; missing, duplicate, list-valued, malformed and prefix-lookalike values are rejected
without logging their contents. GET retains the SDK's exact `application/json`-labelled stream compatibility.

Ordinary JSON success and error bodies have a separate **16 MiB** limit, checked against actual chunks
before accumulation and complete parsing, not only `Content-Length`. Valid 2 MiB JSON remains supported
when the relay's SSE event limit is 1 MiB. Overflow or malformed/truncated request responses fail without
returning partial JSON, replaying the request or changing routes. Route cancellation remains active during
body reads. This is connector policy, not a provider limit, total-process memory cap, universal response
deadline or guarantee of cancelling work already running remotely.

See [verification evidence and safety boundaries](docs/verification-evidence.md) for SDK explicit-rejection
recovery, package provenance, skill trace limitations and retained atomic-temporary recovery policy.

## Tests and CI

For an authorized local light check or a verified CI fallback, the no-argument wrapper runs serial
`cargo test --locked --all-targets -- --test-threads=1`. Its outer limits are 15 minutes wall / 3 minutes
idle; it does not add Nextest case isolation or JUnit output. Do not duplicate the final CI suite locally.
For exact CI runner parity, install the documented cargo-nextest prerequisite separately and use PowerShell:

```bash
pwsh -NoProfile -Command "& ./scripts/verify.ps1 -CargoArgs @('nextest', 'run', '--locked', '--all-targets', '--profile', 'ci', '--no-fail-fast')"
```

The bounded owner terminates its process tree on expiry.
Only the Nextest CI profile uses two workers, 15-second case timeout, 2-second termination grace and 120-second suite timeout,
without retries. CI covers Windows x64, Linux x64, macOS ARM64 and macOS Intel: formatting, Clippy,
UTF-8/source inventory, all suites, native build/launcher smoke, RustSec audit and CodeQL.

Packaging uses an explicit allowlist, verifies extracted binaries from a fresh space-containing folder
and unrelated CWD without a launcher, and checks generated host configuration, blocked startup, version,
settings and SHA256. Only green exact-commit assets are released; private profiles, credentials, logs,
caches and test keys are excluded. Artifacts expire after 14 days; Releases are durable.
Dependabot checks Cargo dependencies and GitHub Actions. CI starts only by manual dispatch; that final batch includes CodeQL
and explicit base/head dependency review. The CodeQL workflow also supports a manual analysis-only
run to establish or repair the default-branch baseline without rebuilding release assets or rerunning
the native test matrix. Its analysis category is stable across standalone and reusable invocations.
Linux CI also runs the
retained native CLI and initialization probes against its checksum-verified packaged executable:
272 CLI assertions, 30 sign-in contracts, 20 raw/framing cases, three credential coordination cases,
eight ordering cases for each entry point, 20 gated initialization-ping cases, 21 bounded-state cases including four valid controls, an 8,192-profile import/reimport scaling case, two route-revision cases and 28 HTTP media/body cases across eleven native suites, plus causal-oracle controls and five ordinary POST/listening-GET cases.
The Rust acceptance controller under `tests/native/` uses isolated loopback TLS and child-only trust, never real
account state. Each probe has a 180-second controller deadline; synthetic keys and binaries are
removed, while sanitized JSON receipts and UTC diagnostic logs are retained as verification artifacts.

Controlled localhost fixtures cover HTTP/HTTPS/SOCKS authentication/DNS, refusal/no fallback,
OAuth exchange/refresh/CSRF/issuer rejection, relay/route changes, malformed config/import, atomic writes,
menu, registration and log safety. TLS keys are intentionally public localhost fixtures.
These tests do not prove live Upwork access, browser interaction or host hot reload; no marketplace writes occur.
See [migration contracts](docs/migration.md) for parity and rollback evidence.

## License

Copyright (C) 2026 KiaroSama. Original code/guidance is **GPL-3.0-only**; see [LICENSE](LICENSE).
Provided without warranty. Dependencies retain their licenses and are restored from Cargo rather than
vendored; preserve required notices when redistributing. RMCP is Apache-2.0; Reqwest is MIT/Apache-2.0.
[Upwork's official plugin](https://github.com/upwork/upwork-agent-plugin) supplies optional skills and the
hosted endpoint declaration, not this connector's implementation.

## Donate

If this project helps you, donations are appreciated.

| Currency | Network | Address |
| --- | --- | --- |
| Bitcoin (BTC) | Bitcoin | `bc1qmth5m03pu5hujw5xw5jmywam3jj3sqwqupesdt` |
| USDT, BNB, USDC, etc. | BEP20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| USDT, TRX, USDC, etc. | TRC20 | `TWBA3xFTqgZAeAYMxqo85xWnzvty3DcAhw` |
| Ethereum (ETH) | ERC20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| TON | TON | `UQCN8Umo_OfOWqImZetQsrNStPcmLkMAKajFyiCOhso23NDb` |
| Litecoin (LTC) | LTC | `ltc1qntqnnrunadurnw4cshv3qgspywrueyyeyngwuy` |
| Solana (SOL) | Solana | `7B2wkczUjmkDhETwQuknBL8sUsbuV7nErxc317TmQuwR` |
| Polygon (POL) | Polygon | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
