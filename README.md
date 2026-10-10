# Upwork Agent Plugin

<div align="center">

[![CI](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/KiaroSama/Upwork-Agent-Plugin/actions/workflows/ci.yml)
[![GPL v3](https://img.shields.io/badge/License-GPL_v3-blue)](LICENSE)
[![Windows 11](https://img.shields.io/badge/Platform-Windows_11-0078d4)](#requirements)
[![Node.js 24.13.1+](https://img.shields.io/badge/Node.js-24.13.1%2B-339933)](#requirements)
[![TypeScript](https://img.shields.io/badge/Source-TypeScript-3178c6)](#implementation)
[![PowerShell 7](https://img.shields.io/badge/Menu-PowerShell_7-5391fe)](#quick-start)
[![mcp-remote 0.14.3](https://img.shields.io/badge/mcp--remote-0.14.3-555555)](https://www.npmjs.com/package/mcp-remote)
[![Undici 8.11.2](https://img.shields.io/badge/Undici-8.11.2-555555)](https://www.npmjs.com/package/undici)
[![MCP stdio](https://img.shields.io/badge/Local_transport-MCP_stdio-555555)](#implementation)
[![Remote HTTP](https://img.shields.io/badge/Upstream_transport-Streamable_HTTP-555555)](https://github.com/upwork/upwork-agent-plugin)
[![HTTP proxy](https://img.shields.io/badge/Proxy-HTTP-555555)](#routing)
[![HTTPS proxy](https://img.shields.io/badge/Proxy-HTTPS-555555)](#routing)
[![SOCKS5 proxy](https://img.shields.io/badge/Proxy-SOCKS5_remote_DNS-555555)](#routing)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

Local connector for **Upwork's hosted MCP**, not an offline or self-hosted Upwork backend. Dependencies, proxy profiles, auth storage, caches and logs belong to this project. Existing Node and PowerShell installations are reused, not installed globally.

## Quick start

Clone or download this repository, open its folder in PowerShell 7, and restore the pinned dependencies:

```bash
npm ci --ignore-scripts --cache ./.cache/npm
```

Before saving credentials, protect the local state directories on Windows:

```powershell
pwsh -NoProfile -File ./scripts/protect-state.ps1 -ProjectRoot .
```

Register the connector for this Claude Code project:

```bash
node src/manage.ts register
```

Open the menu:

```powershell
pwsh -NoProfile -File ./run.ps1
```

1. Choose **2 — Import proxy list from file**. Enter the path with or without outer quotes; Enter uses `Proxy list.txt` in this folder if you have placed your own list there; no real proxy list is distributed. Alternatively choose **3** to add one URL manually.
2. Choose **1 — Select proxy**, then its displayed number. Display names such as `UK 5` are preserved. Initial mode is **blocked**; importing a file does not select a route or connect.
3. Choose **6 — Sign in** yourself. The terminal prints a complete authorization link. Copy it into your browser on this computer; keep the terminal open for the localhost callback. The helper confirms that the session was saved and stops automatically. Do not paste the link or callback into chat.
4. Start a **new Claude Code session in this folder**, approve `upwork-local` if prompted, and reconnect it. Existing sessions may retain the old server registration until reopened.

**4 — Direct** deliberately bypasses proxy routing. **5 — Block** disables connector networking. **7 — Register** refreshes project registration after moving the folder or changing Node's location. No global Claude configuration is changed. The installed plugin's skills remain available; its direct remote MCP entry is denied for this project.

## Proxy list format

One record per UTF-8 line (BOM, blank lines and `#` comments accepted):

```text
socks5://proxy.example.com:12004:example-user:example-password:UK 5
http://proxy.example.com:8080:example-user:example-password:UK 1
```

These are non-secret examples. The **last field is the proxy display name**, including spaces; the preceding fields are host, port, username and password. Standard proxy URLs are also accepted. Colon-delimited credentials cannot themselves contain literal colons; use a standard URL with percent-encoded userinfo for those.

Imports append profiles and skip identical normalized destinations (including credentials). Existing settings and the active route stay unchanged. One invalid line rejects the whole import; errors show line numbers, not private contents. File paths containing spaces and outer quotes work. The input is never modified or published. `0` returns from submenus, `exit` quits at any prompt; Enter in the main menu opens proxy selection.

## Routing

- HTTP and HTTPS use CONNECT tunnels. SOCKS5 sends destination hostnames to the proxy; `socks5h://` is accepted as an alias.
- Example non-secret URLs: `http://127.0.0.1:7890`, `https://proxy.example.com:8443`, `socks5://127.0.0.1:1080`.
- The selected proxy is mandatory for all connector-owned MCP, OAuth discovery, registration, token and refresh requests. Proxy failure produces an error, **never direct fallback**. `NO_PROXY` cannot bypass it.
- A configuration change stops the old connection; reconnect MCP to apply the selected route. In-flight work is interrupted, not replayed automatically. The dispatcher checks configuration before each request; active streams are stopped when the watcher observes the change.
- This does **not** route Claude's model traffic, other MCPs, or your browser. Browser authorization uses the browser's own network settings. The hosted Upwork service still receives requests.
- Undici SOCKS5 support is experimental. Proxy-host DNS may be local; destination DNS is delegated to the proxy.

## Requirements

Existing **Node.js >=24.13.1**, **PowerShell 7**, network access to the selected proxy, and a user-authorized Upwork account. Authenticated tool discovery is not verified until you complete sign-in. No marketplace write operations are part of installation.

## Implementation

TypeScript runs with Node's native type stripping; no build or global package is needed. The pinned `mcp-remote` package handles protocol/OAuth, and the local bootstrap selects the Undici dispatcher. A process-local module resolver blocks automatic browser opening in normal MCP mode. Human-run Sign in prints the complete authorization URL instead of opening any browser. A bounded supervisor sends MCP initialization and requires a usable token file before claiming success.

Project registration writes the local connector entry and a project-only denial of direct `https://mcp.upwork.com/mcp` entries. Registration alone is not proof that the running host loaded it. Claude's own session/trust state remains managed by Claude outside this connector; the connector's new files stay here.

Dependencies can be restored using the lockfile:

```bash
npm ci --ignore-scripts --cache ./.cache/npm
```

Then choose Register from the menu. Saved profiles and account sessions are local runtime data, not part of the public source. A fresh clone starts blocked even without a configuration file.

### Agent control and Claude Desktop

An agent with local shell access can use the same CLI; this is not an extra MCP tool:

```bash
node src/manage.ts status
```

```bash
node src/manage.ts select PROFILE_NAME
```

Selection uses the internal `name` shown by `status`; display labels may contain spaces. Reconnect the MCP host after any route change. `direct` and `block` are explicit CLI commands, never automatic fallback.

For Claude Desktop, run Register and copy the generated `mcpServers.upwork-local` object into Desktop's `mcpServers` configuration. Its `command`, `args` and `env` already contain the paths for your clone; omit the Code-only `type` field if Desktop requires it. Restart Desktop. Registration does not edit global Desktop configuration. The project-only denial of the official direct MCP applies to Claude Code in this project, not to separate Desktop or claude.ai connectors; disable any duplicate direct Upwork connector in that host yourself if you require routed-only access.

## Logs and private state

`logs/` contains one UTF-8 file per executable run, named `upwork-bridge_YYYY-MM-DD_HH-mm-ss_UTC.log` or `upwork-menu_YYYY-MM-DD_HH-mm-ss_UTC.log`; collisions get a suffix. Entries use `[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message`. INFO, WARNING and ERROR record startup, safe route metadata, changes, failures and exit codes. Logging initialization failure falls back to safe stderr.

Upstream verbose/debug output is intentionally disabled: it can contain OAuth URLs, tokens or account payloads. No `--debug` passthrough exists. Safe fixed error codes and discovery/connection stages are recorded; the copyable authorization link is shown only in the human sign-in terminal, never in file logs. Logs contain only fixed safe events; do not attach credential files or proxy settings to support reports. There is no automatic retention/deletion policy; remove old diagnostic logs manually when no longer needed.

Private config, auth and log directories receive owner/SYSTEM-only Windows permissions during setup. Runtime credentials are ignored and must never be published. A browser success page alone confirms receipt of a callback, not successful token exchange: wait for the terminal’s saved-session message. If the callback is blocked, allow localhost/127.0.0.1 directly in your browser proxy configuration only; connector traffic still uses its selected proxy. A moved folder may inherit different ACLs: run `scripts/protect-state.ps1 -ProjectRoot <folder>` before storing secrets. No external accounts or auth credentials were copied from another installation.

## Checks

```bash
node scripts/verify.ts
```

Native tests have a 10-second case timeout and one worker. The outer owner enforces 60 seconds wall / 20 seconds idle and reaps its child on expiry. Controlled localhost proxy/TLS fixtures cover routing, refused proxy, native/upstream fetch compatibility, profile persistence, browser gate and route-change termination. They do not prove real Upwork account access or host hot reload. TLS fixtures are intentionally public test-only keys, never used for production.

GitHub Actions runs the complete discovered test suite on Windows with Node.js 24, from a clean checkout, plus UTF-8/native syntax/PowerShell parse checks and `npm audit`. The job is bounded to 10 minutes. Tests use disposable project-owned state and localhost fixtures, never a personal session or real proxy. Dependabot checks npm and GitHub Actions updates; pull requests also receive dependency review. CodeQL analyzes the public TypeScript source. No compilation build or TypeScript type-check is configured: native stripping does not validate types.

## License and provenance

Copyright (C) 2026 KiaroSama. Original project code is licensed under **GPL-3.0-only**; see [LICENSE](LICENSE). It is provided without warranty.

This is an independent connector, not the official Upwork plugin or a self-hosted Upwork service. [Upwork's official plugin](https://github.com/upwork/upwork-agent-plugin) supplies optional agent skills and declares the hosted endpoint; its source/skills are not bundled here. [mcp-remote](https://github.com/punkpeye/mcp-remote) provides transport and OAuth, and [Undici](https://github.com/nodejs/undici) provides HTTP/proxy dispatch. Both dependencies are MIT-licensed and retain their own licenses when installed. This repository publishes original integration code and public test fixtures, not third-party dependency copies. Installing dependencies retrieves their notices with the packages; preserve those notices when redistributing them.

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
