# Upwork Agent Plugin

<div align="center">

[![CI](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml)
[![GPL v3](https://img.shields.io/badge/License-GPL_v3-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/Source-Rust-dea584)](#implementation)
[![Rust 1.99+](https://img.shields.io/badge/Build-Rust_1.99%2B-dea584)](#optional-source-build-developers-only)
[![Windows](https://img.shields.io/badge/Platform-Windows_11-0078d4)](#installation)
[![Linux](https://img.shields.io/badge/Platform-Linux-555555)](#installation)
[![macOS](https://img.shields.io/badge/Platform-macOS-555555)](#installation)
[![PowerShell 7](https://img.shields.io/badge/Launcher-PowerShell_7-5391fe)](#installation)
[![RMCP](https://img.shields.io/badge/RMCP-3.5.1-555555)](https://docs.rs/rmcp/3.5.1)
[![Reqwest](https://img.shields.io/badge/Reqwest-0.13.5-555555)](https://docs.rs/reqwest/0.13.5)
[![MCP stdio](https://img.shields.io/badge/Local_transport-MCP_stdio-555555)](#desktop-entry)
[![HTTP proxy](https://img.shields.io/badge/Proxy-HTTP-555555)](#routing)
[![HTTPS proxy](https://img.shields.io/badge/Proxy-HTTPS-555555)](#routing)
[![SOCKS5 proxy](https://img.shields.io/badge/Proxy-SOCKS5_remote_DNS-555555)](#routing)
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

| Machine | Download |
| --- | --- |
| Windows x64 | [upwork-agent-plugin-2.0.0-windows-x64.zip](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.0.0-windows-x64.zip) |
| Linux x64, glibc 2.35+ | [upwork-agent-plugin-2.0.0-linux-x64.tar.gz](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.0.0-linux-x64.tar.gz) |
| macOS Apple Silicon | [upwork-agent-plugin-2.0.0-macos-arm64.tar.gz](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.0.0-macos-arm64.tar.gz) |
| macOS Intel | [upwork-agent-plugin-2.0.0-macos-x64.tar.gz](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.0.0-macos-x64.tar.gz) |

Each archive contains the executable in `bin/`, its launchers, skills plugin and GPL license.
`release.json` records the version, platform and source commit; corresponding source is available at
the release tag. Extract the **whole archive** into a new private, writable folder and keep the layout.
Do not move only the executable or overwrite existing profiles, saved sessions, input files or logs.

Verify SHA256 before running. On Windows, from your downloads folder:

```bash
powershell.exe -NoProfile -Command "Get-FileHash ./upwork-agent-plugin-2.0.0-windows-x64.zip -Algorithm SHA256"
```

Compare the displayed hash with the matching `.sha256` file. On Linux:

```bash
sha256sum -c upwork-agent-plugin-2.0.0-linux-x64.tar.gz.sha256
```

On macOS Apple Silicon (use the x64 filename on Intel):

```bash
shasum -a 256 -c upwork-agent-plugin-2.0.0-macos-arm64.tar.gz.sha256
```

Alternatively, download the platform artifact from a successful exact-commit
[CI run](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml) while retained
(14 days; GitHub sign-in may be required). Extract the outer artifact ZIP first, then the platform
archive. Releases provide durable public downloads.

The binaries are not Authenticode-signed or Apple-notarized. Verify source/checksum and follow your OS
security policy rather than disabling protections. Windows ARM, Linux ARM, Alpine/musl and other CPU
targets are not covered by these bundles.

### 2. Run on Windows

Open the extracted folder. Protect private state before saving profiles or signing in:

```bash
powershell.exe -NoProfile -File ./scripts/protect-state.ps1 -ProjectRoot .
```

PowerShell 7 is needed only for the optional launcher/developer scripts:

```bash
pwsh -NoProfile -File ./run.ps1
```

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
tar -xzf upwork-agent-plugin-2.0.0-linux-x64.tar.gz -C upwork-agent-plugin
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
4. For Claude Code, choose **7 — Register**, reopen the project and approve/reconnect `upwork-local`.
   For Desktop, use the entry below. Registration preserves other servers and refuses an unrelated
   existing `upwork-local` entry.

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
    "upwork-local": {
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
requests also check selection. Interrupted work is not replayed automatically. Reconnect the MCP host.
An authorization-server mismatch fails closed without deleting the original session.

Proxy-side DNS prevents local destination lookups, but cannot guarantee a resolver in the proxy exit's
country. Configure upstream DNS/DoH in your local proxy application. Browser DNS/routing/timezone,
Claude model traffic and other MCP servers are outside this connector. No timezone header or field is
created; UTC log/expiry timestamps are not fingerprint spoofing. Caller-owned MCP fields are forwarded unchanged.

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
Import never changes the active route or modifies the input file.

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
no extra management MCP tools are introduced. Reconnect after route changes; never put credentials in arguments.

## Manual OAuth

```bash
./bin/upwork-connector.exe sign-in
```

The official SDK handles protocol/OAuth state machines. The human copies the complete genuine URL into
an external browser on this computer; no automatic browser launch occurs. Keep the terminal open until
usable persisted credentials **and successful MCP initialization** are confirmed. A browser callback
page alone is not proof of completion.

Legacy sessions are read without modifying their files; native refresh/sign-in writes separate Rust state.
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
and install **Upwork Project Selection**. Invoke `$upwork-project-selection`. Surface/version availability
varies; this is not a universal `codex plugin install` command. Standalone copying also works.

### Install the Cursor skill

Download the [skills-only ZIP](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-selection-plugin-2.0.0.zip).
Copy `skills/upwork-project-selection/` to the workspace's `.agents/skills/upwork-project-selection/`.
Preserve existing files, keep only one discovery copy, reopen the workspace and invoke `/upwork-project-selection`.

### Install the Antigravity skill

Use the same original skill directory and workspace `.agents/skills/upwork-project-selection/` path.
Use a real copy, not a symlink, and inspect an existing destination before replacement. Reopen the workspace
and invoke `/upwork-project-selection`. No native Antigravity plugin-install command is claimed here.

### Installation boundaries

See [complete client instructions](docs/skills.md) for collision-safe Windows/Unix copying, manifest details
and verification limits. Actual Claude validator and isolated marketplace installation were exercised;
Codex/Cursor/Antigravity checks are structural/current-documentation evidence, not native runtime proof.
Three synthetic evaluation prompts are not proof of live marketplace success or statistical effectiveness.
Official Upwork skills remain separate; no global installation, permission expansion or duplicate route is enabled automatically.

## Logging

Each run creates a UTF-8 log in `logs/`, such as `upwork-native_YYYY-MM-DD_HH-mm-ss_UTC.log` or
`upwork-bridge_YYYY-MM-DD_HH-mm-ss_UTC.log`; collisions receive a suffix. Entries use
`[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message`. INFO, WARNING and ERROR record fixed safe
startup, callback, failure and shutdown events. File initialization failure falls back to safe stderr.

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
is deliberately disabled because vendor events can expose OAuth secrets.

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
Build/verification scripts keep generated paths inside the project. Preserve `bin/`, profiles, session,
input files and operational logs when clearing optional development artifacts; shared tools remain untouched.

## Tests and CI

```bash
pwsh -NoProfile -File ./scripts/verify.ps1
```

The bounded owner limits Cargo to 15 minutes wall / 3 minutes idle and terminates its process tree on expiry.
Nextest uses two workers, 15-second case timeout, 2-second termination grace and 120-second suite timeout,
without retries. CI covers Windows x64, Linux x64, macOS ARM64 and macOS Intel: formatting, Clippy,
UTF-8/source inventory, all suites, native build/launcher smoke, RustSec audit and CodeQL.

Packaging uses an explicit allowlist, verifies extracted binaries from a fresh space-containing folder
and unrelated CWD without a launcher, and checks generated host configuration, blocked startup, version,
settings and SHA256. Only green exact-commit assets are released; private profiles, credentials, logs,
caches and test keys are excluded. Artifacts expire after 14 days; Releases are durable.
Dependabot checks Cargo/actions and pull requests receive dependency review.

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
