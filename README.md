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

Clone this repository and open its folder in PowerShell 7. Build with an existing Rust toolchain:

```powershell
pwsh -NoProfile -File ./build.ps1
```

The build restores dependencies from `Cargo.lock`, writes the native executable under `bin/`, and protects local state directories on Windows. It never installs global software automatically.

```powershell
pwsh -NoProfile -File ./run.ps1
```

1. Choose **2 — Import proxy list from file** and enter your UTF-8 file path, with or without quotes. Enter defaults to your own `Proxy list.txt` in this folder; no real proxy list is distributed.
2. Choose **1 — Select proxy**, then its displayed number. Initial mode is **blocked**; import alone does not select a route or connect.
3. Choose **6 — Sign in** yourself. Copy the complete authorization URL into your browser on the same computer. Keep the terminal open for the localhost callback; wait for the terminal's saved-session and MCP-initialization confirmation.
4. Choose **7 — Register** for Claude Code, reopen this project and approve/reconnect `upwork-local`.

`0` returns from submenus, `exit` quits at every prompt, and Enter in the main menu opens selection. **4 — Direct** deliberately bypasses proxies; **5 — Block** disables connector networking.

### macOS and Linux

With an existing Rust 1.99+ toolchain and native linker, no PowerShell is required:

```bash
sh ./build.sh
```

```bash
sh ./run.sh
```

The shell launcher resolves its own folder, supports paths containing spaces and uses private file permissions. Generate Desktop configuration where a supported Desktop client is available:

```bash
sh ./run.sh desktop-config
```

The same stdio executable works with MCP hosts on Linux; availability of a particular host application is separate from connector support.

## Claude Desktop

Generate the exact configuration for your executable and folder:

```powershell
./bin/upwork-connector.exe --root . desktop-config
```

Merge the printed `mcpServers.upwork-local` object into `claude_desktop_config.json` without removing other servers, then restart Desktop. The `command` points directly to the native executable, and `args` explicitly bind state to this folder; do not replace it with `cargo run`, Node or a hosted HTTP entry.

Registration only edits Claude Code's project configuration. Its direct-Upwork denial does **not** apply to separate Desktop or claude.ai connectors. Disable duplicate direct Upwork connectors in that host yourself if you require routed-only access. Optional official Upwork skills can remain installed; they are not bundled here.

## Agent and CLI control

An agent with local shell access uses the same native CLI; no extra management MCP tools are introduced:

```powershell
./bin/upwork-connector.exe --root . status
```

```powershell
./bin/upwork-connector.exe --root . select UK-1
```

`select` uses the internal `name` returned by `status`, not a spaced display label. Other commands are `import PATH`, `add` (JSON from stdin), `direct`, `block`, `sign-in`, `bridge`, `register`, `desktop-config` and `preview`. Reconnect MCP after changing a route. Credential-bearing URLs never belong in command arguments.

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

**Build:** Rust 1.99 or newer, Cargo, platform linker/build tools (MSVC on Windows), and PowerShell 7 for the supplied scripts. **Run:** the built native executable, access to the selected proxy and a user-authorized Upwork account; no Rust/Node runtime is needed to execute the binary. Core tests run natively on Windows, macOS and Linux. The Windows ACL helper is Windows-only.

## Implementation

Rust 2024 modules own profiles, file import, routing, credentials, OAuth, MCP relay, logging and the menu. The official [RMCP SDK](https://github.com/modelcontextprotocol/rust-sdk) implements protocol and OAuth state machines; [Reqwest](https://github.com/seanmonstar/reqwest) supplies the explicitly configured HTTP/proxy clients. Both OAuth redirect policies use routed clients; SDK defaults are never used for outbound requests.

The relay forwards JSON-RPC protocol messages rather than enumerating or reimplementing Upwork tools. SDK debug tracing is deliberately disabled because its events can contain OAuth secrets. Manual sign-in prints a link, never launches an OS browser. Success requires both usable persisted credentials and a successful MCP initialization response.

See [migration notes](docs/migration.md) for state compatibility, parity fixtures and rollback. Saved legacy sessions are read without modifying their files; native refresh/sign-in writes separate Rust state.

## Logs and private state

`logs/` receives one UTF-8 log per executable run, with names such as `upwork-native_YYYY-MM-DD_HH-mm-ss_UTC.log` and `upwork-bridge_YYYY-MM-DD_HH-mm-ss_UTC.log`; collisions receive a suffix. Entries use `[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message`. INFO, WARNING and ERROR describe fixed safe startup, callback, failure and shutdown events. File initialization failure falls back to safe stderr.

No raw upstream errors, account payloads, tokens, passwords or authorization links are logged. The complete login URL appears only in the explicit human sign-in terminal. There is no debug passthrough or automatic log retention. Attach only reviewed diagnostic logs to support requests, never profile or credential files.

Build setup applies owner/SYSTEM-only permissions to config, credentials and log directories on Windows. After moving the project, restore those permissions before saving secrets:

```powershell
pwsh -NoProfile -File ./scripts/protect-state.ps1 -ProjectRoot .
```

A browser callback page alone is not proof of completion. Wait for the terminal confirmation. If your browser proxy blocks localhost, exempt localhost/127.0.0.1 in **browser settings only**; connector-owned requests still use their selected route.

## Checks

```powershell
pwsh -NoProfile -File ./scripts/verify.ps1
```

The bounded owner limits Cargo commands to 15 minutes wall / 3 minutes idle and terminates the owned process tree on expiry. Tests have explicit network/CLI deadlines. CI uses Nextest with two workers, 15-second case timeout, 2-second termination grace and 120-second suite timeout, without retries. The final CI matrix covers Windows, macOS and Linux, Clippy, formatting, strict UTF-8/source inventory, all test suites, native build/launcher smoke, RustSec dependency audit and CodeQL. Dependabot checks Cargo and action updates; pull requests receive dependency review.

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
