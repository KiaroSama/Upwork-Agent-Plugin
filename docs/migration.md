# Native Rust migration

Version 2 replaces the Node connector with a native Rust executable. Rust owns profile persistence, proxy routing, OAuth, token refresh, MCP relay and the terminal menu. PowerShell is only the Windows build/launch adapter. No Node subprocess or `mcp-remote` runtime is used.

## 1. Preserved contracts

- The profile JSON schema, blocked/direct/proxy states, labels and CLI operations remain compatible.
- The proxy importer preserves the last field as the display label, rejects invalid UTF-8 and commits no partial imports.
- Every OAuth and MCP request uses an explicitly configured Reqwest route. Both OAuth redirect-following and no-redirect clients share that route; SDK default clients are not used. SOCKS5 is mapped to SOCKS5h for remote destination DNS.
- The relay uses the official Rust SDK transport and preserves JSON-RPC messages instead of hardcoding Upwork tools.
- Diagnostic logs contain fixed application events only. SDK tracing is not enabled because upstream debug events may contain OAuth parameters.

## 2. Existing sessions

The native credential store can read the previous client registration and token files without modifying them. Millisecond token expiry is converted to the SDK's epoch-second receipt timestamp. Native refresh/sign-in writes separate Rust state. An authorization-server mismatch fails closed rather than deleting the original credentials.

A saved token is not proof of working live access. Manual sign-in verifies MCP initialization before reporting completion. Browser traffic remains outside the connector's routing guarantee.

## 3. Parity and validation

`tests/fixtures/config-oracle.json` was generated from commit `3748197` using Node 24.21.0 before retirement. It validates structural configuration/status parity. UTC timestamps, machine paths and safe error phrasing are intentionally not byte-exact.

Native tests cover persistence/rollback/locking, UTF-8/import encoding/deduplication, localhost HTTP/HTTPS/SOCKS routing and refusals, OAuth exchange/refresh/CSRF/issuer checks, MCP relay and route changes, credential/log protection, menu and registration. Only synthetic non-secret localhost data enters CI; no live marketplace writes or mirrored account traffic.

CI runs format, Clippy, bounded Nextest, native build/launcher smoke, source/UTF-8 inventory, dependency audit and CodeQL on the supported platforms. A test cannot be silently skipped to make cutover green.

## 4. Retirement and rollback

After the Rust checks pass, the old implementation and its project-owned dependencies/cache/temp are removed. Saved proxies, user input, credential state and real operational logs are not disposable software caches. Shared system runtimes and unrelated project state are not touched.

Commit `3748197` remains the historical rollback source in Git; no old runtime is needed by the native executable. Restoring it requires deliberately checking out that revision and restoring dependencies, never overwriting current user state.
