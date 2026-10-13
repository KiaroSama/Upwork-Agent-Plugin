# Local TLS fixture

The localhost-test certificate and key are intentionally public, generated solely for deterministic localhost proxy/target TLS tests. They never authorize an account or production service. Certificate SAN covers localhost and 127.0.0.1; Native Rust tests use an isolated certificate-only trust store. Production connector never installs or references it. Generated using OpenSSL; valid for ten years from 2026-10-10.

## Configuration oracle provenance

`config-oracle.json` originated from the historical Node implementation recorded in its `source` field. It is not regenerated with Node during Rust CI. Intentional contract corrections must be recorded in its `amendments` field rather than represented as unchanged historical output.

The 2026-10-13 R04 correction changes only the sanitized HTTP default-port expectation from `http://proxy.example/` to `http://proxy.example:80/`. The old output is rejected by the connector's explicit-port input validator. Retaining the port fixes the public getter/setter round trip; no private profile values are rewritten. Dedicated command-boundary tests independently cover HTTP/HTTPS default ports, nondefault ports, IPv4, IPv6 and credential removal. All original validation cases and the exact oracle comparison remain enabled.
