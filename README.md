# Upwork Agent Plugin

<div align="center">

[![CI](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml)
[![GPL v3](https://img.shields.io/badge/License-GPL_v3-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/Source-Rust-dea584)](#implementation)
[![Rust 1.99+](https://img.shields.io/badge/Build-Rust_1.99%2B-dea584)](#requirements)
[![Windows](https://img.shields.io/badge/Platform-Windows_11-0078d4)](#requirements)
[![Linux](https://img.shields.io/badge/Platform-Linux-555555)](#checks)
[![macOS](https://img.shields.io/badge/Platform-macOS-555555)](#checks)
[![PowerShell 7](https://img.shields.io/badge/Launcher-PowerShell_7-5391fe)](#quick-start)
[![RMCP](https://img.shields.io/badge/RMCP-3.5.1-555555)](https://docs.rs/rmcp/3.5.1)
[![Reqwest](https://img.shields.io/badge/Reqwest-0.13.5-555555)](https://docs.rs/reqwest/0.13.5)
[![MCP stdio](https://img.shields.io/badge/Local_transport-MCP_stdio-555555)](#implementation)
[![HTTP proxy](https://img.shields.io/badge/Proxy-HTTP-555555)](#routing)
[![HTTPS proxy](https://img.shields.io/badge/Proxy-HTTPS-555555)](#routing)
[![SOCKS5 proxy](https://img.shields.io/badge/Proxy-SOCKS5_remote_DNS-555555)](#routing)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

Native Rust connector for **Upwork's hosted MCP**, not an offline or self-hosted Upwork backend. This independent project is not the official Upwork plugin. It provides a terminal menu, saved proxy profiles and fail-closed routing. Node.js and `mcp-remote` are not runtime dependencies.

## Quick start

**Download a ready-to-run bundle from [Releases](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest). No Rust, Cargo, Node.js or compiler installation is needed.** Download the archive matching your OS/CPU, not GitHub's source-code ZIP.

| Platform | Ready-to-run download |
| --- | --- |
| Windows x64 | [Windows ZIP](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.0.0-windows-x64.zip) |
| Linux x64, glibc 2.35+ | [Linux tar.gz](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.0.0-linux-x64.tar.gz) |
| macOS Apple Silicon | [macOS ARM64 tar.gz](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.0.0-macos-arm64.tar.gz) |
| macOS Intel | [macOS x64 tar.gz](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-agent-plugin-2.0.0-macos-x64.tar.gz) |

Each archive has a matching `.sha256` checksum asset and `release.json` identifying its version, platform and source commit. The binaries are not Authenticode-signed or Apple-notarized; review the source/checksum and your OS warning rather than disabling system protections. Linux ARM, Windows ARM, Alpine/musl and other CPU targets are not covered by these native bundles.

### Windows installation

1. Download the Windows ZIP and its `.sha256` asset. Verify the ZIP's SHA256 against that file:

```powershell
Get-FileHash ./upwork-agent-plugin-2.0.0-windows-x64.zip -Algorithm SHA256
```

2. Extract the **whole archive** into a writable folder; keep `bin/`, `scripts/`, `skills/` and the launchers together. Open that folder in PowerShell. Protect private state before saving profiles or signing in:

```powershell
./scripts/protect-state.ps1 -ProjectRoot .
```

3. Run the native menu directly; PowerShell 7 is optional for this command:

```powershell
./bin/upwork-connector.exe menu
```

The optional `run.ps1` launcher uses PowerShell 7. Extract into a new folder for upgrades, or replace only shipped program files; preserve your existing profile/credential/log directories. Never extract another person's saved state.

### Linux and macOS installation

1. Download the matching tar.gz and `.sha256` assets. On Linux verify with `sha256sum`; macOS uses `shasum -a 256`:

```bash
sha256sum -c upwork-agent-plugin-2.0.0-linux-x64.tar.gz.sha256
```

```bash
shasum -a 256 -c upwork-agent-plugin-2.0.0-macos-arm64.tar.gz.sha256
```

2. Create a private, writable installation folder and extract the complete archive there:

```bash
mkdir -m 700 upwork-agent-plugin
```

```bash
tar -xzf upwork-agent-plugin-2.0.0-linux-x64.tar.gz -C upwork-agent-plugin
```

Substitute your macOS archive name when applicable. The tar preserves executable permissions. Run from the extracted folder, or pass its path to the launcher:

```bash
sh ./upwork-agent-plugin/run.sh
```

The same stdio executable works with MCP hosts on Linux; a particular host application's availability is separate from connector support.

### Choose a route and sign in

1. **Local proxy:** choose **8 — Local proxy settings** to match your existing local proxy (default `socks5h://127.0.0.1:1595`), then **9 — Select local proxy**. This does not install a proxy service.
2. **Saved remote proxy:** choose **2 — Import proxy list from file**, enter its UTF-8 path, then **1 — Select proxy**. Initial mode is blocked; importing does not enable networking.
3. Choose **6 — Sign in** yourself. Copy the complete authorization URL into your browser on the same computer. Keep the terminal open for the localhost callback and wait for saved-session/MCP-initialization confirmation.
4. For Claude Code, choose **7 — Register**, reopen the extracted project folder in Claude and approve/reconnect `upwork-local`. For Desktop, follow the configuration section below.

`0` returns from submenus, `exit` quits, and Enter in the main menu opens selection. **4 — Direct** deliberately bypasses proxies; **5 — Block** disables connector networking. Sign-in and connector registration are separate from installing the advisory skills plugin.

## Claude Desktop

Generate the exact configuration for your executable and folder:

```powershell
./bin/upwork-connector.exe --root . desktop-config
```

Merge the printed `mcpServers.upwork-local` object into `claude_desktop_config.json` without removing other servers, then restart Desktop. The `command` points directly to the native executable, and `args` explicitly bind state to this folder; do not replace it with `cargo run`, Node or a hosted HTTP entry.

`--root` binds connector state, not the operating system's temporary-directory environment. Generated Desktop and project registration entries also include `env.TEMP`, `env.TMP` and `env.TMPDIR` pointing to `<installation>/.cache/tmp`, plus `env.XDG_CACHE_HOME` pointing to `<installation>/.cache`. The executable creates these private directories; the launchers apply the same per-process environment. The connector has no persistent HTTP response cache. Its profiles, account state and logs remain under the installation folder; atomic write scratch files stay beside their destination. This does not relocate Claude Desktop's own model/cache files or the separate browser's data.

Registration only edits Claude Code's project configuration. Its direct-Upwork denial does **not** apply to separate Desktop or claude.ai connectors. Disable duplicate direct Upwork connectors in that host yourself if you require routed-only access. Optional official Upwork skills can remain installed; they are not bundled here.

## Skills plugin installation

The bundled [upwork-project-selection](skills/upwork-project-selection/SKILL.md) skill evaluates buying intent, freelancer fit and contract value before investing time or Connects. It is advisory/read-only; installation is independent from the native MCP connection.

**Claude Code:** add the marketplace and install:

```bash
claude plugin marketplace add KiaroSama/Upwork-Agent-Plugin
```

```bash
claude plugin install upwork-agent-connector@upwork-agent-plugin-marketplace
```

Invoke `/upwork-agent-connector:upwork-project-selection` in a new session.

**Codex:** register the catalog:

```bash
codex plugin marketplace add KiaroSama/Upwork-Agent-Plugin --ref main
```

Restart the ChatGPT desktop app, open Plugins Directory, choose `upwork-agent-plugin-marketplace` and install **Upwork Project Selection**. Invoke `$upwork-project-selection`. Standalone skill copying also works.

**Cursor / Antigravity:** download the [skills-only ZIP](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest/download/upwork-selection-plugin-2.0.0.zip), extract it, and copy `skills/upwork-project-selection/` into your workspace's `.agents/skills/` without overwriting an existing skill. Reopen the workspace and invoke `/upwork-project-selection`.

See [complete client instructions](docs/skills.md) for collision-safe Windows/Unix commands, local Claude loading and verification limits. One canonical skill is shared by portable/Claude/Codex/Cursor manifests. Installation neither replaces official Upwork skills nor enables a duplicate direct MCP route.

## Agent and CLI control

An agent with local shell access uses the same native CLI; no extra management MCP tools are introduced:

```powershell
./bin/upwork-connector.exe --root . status
```

```powershell
./bin/upwork-connector.exe --root . select UK-1
```

`select` uses the internal `name` returned by `status`, not a spaced display label. Other commands are `import PATH`, `add` (JSON from stdin), `direct`, `block`, `sign-in`, `bridge`, `register`, `desktop-config` and `preview`. Reconnect MCP after changing a route. Credential-bearing URLs never belong in command arguments.

## Local proxy settings

**8 — Local proxy settings** edits the endpoint; default is `socks5h://127.0.0.1:1595`. Change its scheme, host or explicit port to match your existing local proxy. **9 — Select local proxy** applies it; reconnect MCP afterward. No local proxy service is installed or started. Changing settings alone does not switch routes or rewrite imported profiles, including their ports.

Agents read the sanitized setting with `local-settings`, update it using `local-settings set` with `{"url":"http://127.0.0.1:1595"}` on stdin, and select it using `local-proxy`. Credentials belong only in protected stdin/config, never command arguments. Local settings are independent from OAuth's callback port and Upwork's HTTPS destination port 443.

Remote destination DNS prevents local destination lookups, but does **not** guarantee that a proxy provider's resolver IP is in the same country as its exit IP. For a local proxy, configure its upstream DNS/DoH routing in that proxy application. Browser DNS and browser timezone are outside this connector. The connector creates no timezone header or field; UTC log/expiry timestamps are not timezone fingerprint spoofing. Caller-owned fields inside transparent MCP messages are forwarded unchanged.

## Proxy list format

One record per UTF-8 line, with optional BOM, blank lines and `#` comments:

```text
socks5://proxy.example.com:12004:example-user:example-password:UK 5
http://proxy.example.com:8080:example-user:example-password:UK 1
```

These are non-secret examples. The last field is the display name, including spaces. Standard proxy URLs are also accepted. Colon-delimited credentials cannot contain literal colons; use a standard URL with percent-encoded userinfo instead.

Imports append validated profiles and skip identical normalized URLs, including credentials. One bad line rejects the complete import without partial writes; errors show only the line number. Labels are preserved separately from safe identifiers. Import does not change the active route or modify the input file.

## Routing

- HTTP proxies forward HTTP requests and tunnel HTTPS with CONNECT; HTTPS proxies encrypt the connection to the proxy. SOCKS5/SOCKS5h send destination hostnames to the proxy for remote DNS.
- Every connector-owned MCP, OAuth discovery, registration, token exchange and refresh request uses its selected route. A failed proxy produces an error, **never direct fallback**. Environment/system proxy and `NO_PROXY` settings cannot bypass the explicit route.
- Changing configuration invalidates the route and stops the old connection when the watcher observes it (100 ms interval). New requests also check the selection; interrupted work is not replayed automatically. Reconnect the MCP host to apply the new route.
- This does not route Claude model traffic, other MCP servers or browser traffic. Your browser uses its own network settings, and the Upwork backend remains hosted.
- An authorization-server mismatch fails closed; it does not delete your original session. Explicit Sign in can obtain a new authorization.

## Requirements

**Run:** matching release archive, a writable private installation folder, access to the explicitly selected route and a user-authorized Upwork account. No Rust/Cargo/Node installation is required. Windows x64, Linux x64 (glibc 2.35+) and macOS ARM64/x64 bundles are built and exercised natively in CI. PowerShell 7 is needed only for the optional Windows launcher/developer scripts; Windows PowerShell can run the ACL setup. Browser OAuth remains user-owned.

## Implementation

Rust 2024 modules own profiles, file import, routing, credentials, OAuth, MCP relay, logging and the menu. The official [RMCP SDK](https://github.com/modelcontextprotocol/rust-sdk) implements protocol and OAuth state machines; [Reqwest](https://github.com/seanmonstar/reqwest) supplies the explicitly configured HTTP/proxy clients. Both OAuth redirect policies use routed clients; SDK defaults are never used for outbound requests.

The relay forwards JSON-RPC protocol messages rather than enumerating or reimplementing Upwork tools. SDK debug tracing is deliberately disabled because its events can contain OAuth secrets. Manual sign-in prints a link, never launches an OS browser. Success requires both usable persisted credentials and a successful MCP initialization response.

See [migration notes](docs/migration.md) for state compatibility, parity fixtures and rollback. Saved legacy sessions are read without modifying their files; native refresh/sign-in writes separate Rust state.

## Logs and private state

`logs/` receives one UTF-8 log per executable run, with names such as `upwork-native_YYYY-MM-DD_HH-mm-ss_UTC.log` and `upwork-bridge_YYYY-MM-DD_HH-mm-ss_UTC.log`; collisions receive a suffix. Entries use `[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message`. INFO, WARNING and ERROR describe fixed safe startup, callback, failure and shutdown events. File initialization failure falls back to safe stderr.

No raw upstream errors, account payloads, tokens, passwords or authorization links are logged. The complete login URL appears only in the explicit human sign-in terminal. There is no debug passthrough or automatic log retention. Attach only reviewed diagnostic logs to support requests, never profile or credential files.

Build setup applies owner/SYSTEM-only permissions to config, credentials, cache and log directories on Windows. After moving the project, restore those permissions before saving secrets:

```powershell
pwsh -NoProfile -File ./scripts/protect-state.ps1 -ProjectRoot .
```

A browser callback page alone is not proof of completion. Wait for the terminal confirmation. If your browser proxy blocks localhost, exempt localhost/127.0.0.1 in **browser settings only**; connector-owned requests still use their selected route.

## Developer builds (optional)

Normal users download Releases; only contributors need Rust 1.99+, Cargo and a native linker. Source builds remain available with `build.ps1` on Windows or `build.sh` on Unix. Build output/cache can consume several GB under `target/`; it is not part of a release bundle. CI builds the downloadable binaries, so user installation never needs that cache. Build/verification scripts confine temporary files, Cargo downloads and compiler output to this project. CI also confines its downloaded toolchain and installer/advisory caches to its own checkout. Existing shared tools are not moved or deleted; downloaded runtime bundles contain none of these caches.

## Checks

```powershell
pwsh -NoProfile -File ./scripts/verify.ps1
```

The bounded owner limits Cargo commands to 15 minutes wall / 3 minutes idle and terminates the owned process tree on expiry. Tests have explicit network/CLI deadlines. CI uses Nextest with two workers, 15-second case timeout, 2-second termination grace and 120-second suite timeout, without retries. The final CI matrix covers Windows x64, Linux x64, macOS ARM64 and macOS Intel, Clippy, formatting, strict UTF-8/source inventory, all test suites, native build/launcher smoke, RustSec dependency audit and CodeQL. Each CI build packages an allowlisted archive, verifies the extracted native executable in a fresh space-containing folder, and uploads the archive plus SHA256 checksum. Only green exact-commit artifacts are published to Releases; private profiles, credentials, logs, caches and test keys are excluded. CI artifacts expire after 14 days; Releases provide durable public downloads. Dependabot checks Cargo and action updates; pull requests receive dependency review.

Controlled localhost fixtures test HTTP/HTTPS/SOCKS authentication and remote DNS, proxy refusal/no fallback, OAuth code exchange/refresh and CSRF/issuer rejection, MCP relay/route changes, malformed config/import, atomic writes, menu, registration and log safety. TLS keys are intentionally public localhost-only fixtures. These tests do not prove live Upwork access, browser interaction or host hot reload; no marketplace writes are executed.

## License and provenance

Copyright (C) 2026 KiaroSama. Original code is **GPL-3.0-only**; see [LICENSE](LICENSE). Provided without warranty. Dependencies retain their own licenses and are restored from Cargo rather than vendored. Preserve their notices when redistributing them. RMCP is Apache-2.0; Reqwest is MIT/Apache-2.0. [Upwork's official plugin](https://github.com/upwork/upwork-agent-plugin) supplies optional skills and the hosted endpoint declaration, not this connector's implementation.

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
