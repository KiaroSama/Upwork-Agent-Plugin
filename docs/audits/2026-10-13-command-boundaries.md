# Continuation audit: command dispatch and public proxy URLs

> Historical engagement record, not current instructions or authorization. Imperatives below describe original review requirements, not permission to execute, publish, merge, rewrite history or expand scope. Evidence is baseline-specific. See [current dispositions](README.md).

## Archived engagement requirements and evidence

**First obey the owner's actual private Rules, hooks and prescribed Spec Kit chain.** They are intentionally absent from this public checkout. Their execution is not claimed. This report supplements the import-boundary report in the same PR #5; it does not reopen the resolved A01-A09 findings in merged PR #2.

Baseline: `43b75110177bf0420b45e95a2ec965df4bcfd9ee`. Existing import-fix head before this continuation: `ac3428f9780f8d3fa75cb834f159616a40be41f7`. The assistant only updates the open fix PR; it does not merge or close PRs or change main.

## R03: invalid invocations can change the selected route

Priority: P1 (explicit-routing contract). `execute` inspected the first argument and most branches ignored the remainder. For example, `direct --help` and `direct --root another-installation` were accepted as direct-mode requests. The latter did not redirect the operation to another root: root parsing accepts the global option only before the command. A typo or unsupported flag could therefore silently replace the selected proxy with Direct in the current installation. The invalid command itself was not observed sending network traffic; subsequent connections would use the persisted mode.

The exact, checksum-verified baseline Linux archive from workflow `38357420087`, artifact `11793910851`, returned exit 0 and persisted `mode=direct` for `direct --not-a-real-option` in an isolated empty installation. No live Upwork requests were made.

Implemented fix: `validated_command` checks the complete positional shape of every supported command before cache creation, logger initialization, maintenance cleanup, stdin parsing, registration, configuration mutation or networking. Unsupported and missing/extra arguments produce a fixed, non-secret error and nonzero exit. Default menu, explicitly selected Direct/Block, and all documented valid command shapes remain accepted. `--root PATH` remains a prefix-only global option; an incorrectly positioned copy is rejected rather than ignored. No new CLI dependency or compatibility-breaking option inference is introduced.

Acceptance tests in `tests/command_boundaries.rs` cover 24 invalid invocations, absence of new filesystem state, byte preservation of an existing selected proxy, and valid Direct/Block/default-menu controls. Invalid sign-in and bridge cases cannot reach their dispatch branches. The test process wrapper has a four-second kill/reap deadline and captures protocol stdout separately from stderr.

## R04: default-port public URL output fails its own input contract

Priority: P2 (configuration round-trip). `proxy_url` deliberately requires an explicit positive port. `Url` serialization, however, removes HTTP port 80 and HTTPS port 443. Both `Connections::status` and `LocalProxy::public_url` previously returned that canonical URL without restoring the port. Thus setting `http://127.0.0.1:80`, reading local settings, and setting the returned JSON failed even without credentials. The same defect affected proxy URLs in status output.

The exact baseline binary exported `http://127.0.0.1/` and `https://127.0.0.1/`; both reapplications returned exit 1. These are observed failures, not guesses about upstream URL behavior.

Implemented fix: a shared `config::public_proxy_url` validates the original URL, obtains the effective explicit-or-default port, removes userinfo, and restores a default port at the URL crate's parsed `AfterHost` boundary when the serializer omits it. This preserves IPv6 brackets and does not rebuild host syntax manually. Nondefault-port output remains unchanged. Both public getters use the helper. Input validation still requires a port; stored private URLs and credentials are not rewritten.

Acceptance tests cover HTTP, HTTPS, SOCKS5 and SOCKS5h, domain/IPv4/IPv6 hosts, removal of username/password, serializer idempotence, getter/setter round trips, nondefault-port controls, and exact-byte preservation of private profile storage. Sanitized output intentionally does not retain authentication credentials; it is not a credential backup.

## Verification and integration gates

The predecessor import-fix head passed CI, CodeQL and Dependency Review. That evidence does not approve this newer head. The full final-head matrix must pass inventory, rustfmt, strict Clippy, all bounded tests, native build, launcher checks, cleanup and packaging on Windows x64, Linux x64 and both supported macOS targets. Require final-head cargo-audit, CodeQL and Dependency Review. Existing Cargo and Actions Dependabot configuration remains enabled; no gate is disabled, weakened or replaced by an earlier result. All new integration tests are auto-discovered by the existing Cargo inventory check and four-platform workflow.

This continuation's local container has no Rust compiler and cannot clone GitHub directly. Source replacements were checked against the exact baseline Git blob hashes. Independent black-box reproductions used the downloaded real binary; source compilation and full regression execution are performed by hosted CI. Final statuses, JUnit counts and downloaded final-binary checks belong in the completion receipt, not fabricated in this report before they exist.

**Historical owner-identity gate:** this engagement required `Kiaro.Sama.Dev@gmail.com` in owner-attributed raw author/committer fields. The connector API exposed no override and selected the account's noreply email. The reviewing agent repaired the five affected owner commits with native Git objects and verified unchanged trees/topology/dates/messages. The corrected source head was `476e71ec1cda2c857f239d4164277d92fde3899e`; replacement-head CI and artifact evidence remained separate gates. Historical remediation required verified recovery, coordination and explicit expected-ref leases; a mailmap or correctly addressed tip did not repair old objects. No global account settings or default-branch history were changed by the auditor. This record neither authorizes a later rewrite nor applies owner attribution to legitimate bots.

The original engagement required current primary-source research, the owner's prescribed Spec Kit sequence, review of the then-open PRs, and evidence-backed integration or supersession. It also required regression coverage for newly reproduced defects and verification after authorized commits/publication. Those were historical acceptance requirements, not standing authority for a later agent to change branches, merge, close, commit or push. The auditing assistant performed none of those merge/close operations.

Optional improvement candidates for the same-session decision record: a small typed command model to prevent future dispatcher/validator drift; property-based URL and import testing; a scheduled read-only dependency audit. Evaluate costs and existing coverage before adoption. These are proposals, not observed defects or grounds for reopening accepted fixes.

## Primary references

- [Rust Read::take](https://doc.rust-lang.org/std/io/trait.Read.html#method.take)
- [Rust File and metadata](https://doc.rust-lang.org/std/fs/struct.File.html)
- [URL port and serialization API](https://docs.rs/url/latest/url/struct.Url.html)
- [MCP stdio framing and stdout boundary](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [GitHub commit author/committer semantics](https://docs.github.com/en/rest/git/commits#create-a-commit)
- [Spec Kit upstream](https://github.com/github/spec-kit)

No live account sign-in, private production proxy, account write, proposal, message or Connects expenditure was performed. No finite audit is represented as proof that all possible defects are absent.
