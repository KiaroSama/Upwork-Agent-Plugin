# Local TLS fixture

The localhost-test certificate and key are intentionally public, generated solely for deterministic localhost proxy/target TLS tests. They never authorize an account or production service. Certificate SAN covers localhost and 127.0.0.1; test runner trusts this certificate only in its child process via NODE_EXTRA_CA_CERTS. Production connector never installs or references it. Generated using OpenSSL; valid for ten years from 2026-10-10.
