# Local TLS fixture

The localhost-test certificate and key are intentionally public, generated solely for deterministic localhost proxy/target TLS tests. They never authorize an account or production service. Certificate SAN covers localhost and 127.0.0.1; Native Rust tests use an isolated certificate-only trust store; the previous Node harness uses NODE_EXTRA_CA_CERTS in its child process. Production connector never installs or references it. Generated using OpenSSL; valid for ten years from 2026-10-10.
