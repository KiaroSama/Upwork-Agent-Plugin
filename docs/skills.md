# Skills and client compatibility

## Skill catalog

| Skill | Purpose | Input | Output |
| --- | --- | --- | --- |
| [upwork-project-selection](../skills/upwork-project-selection/SKILL.md) | Select worthwhile Upwork jobs before investing application time or Connects | Job/client evidence and real freelancer experience/constraints | Pursue, Clarify, Pass or Stop for payment safety; three gates, evidence, economics and next action |

The skill is an original English synthesis of a user-provided two-page Persian project-selection article. It retains ten risk signals, positive buying intent and three preproposal gates. The article's reported 4,281 postings/15 years of experience are unverified observations, not a statistical model or official Upwork policy. The PDF is not distributed. Skill instructions are GPL-3.0-only, like this project.

The skill works with pasted descriptions. Authorized read-only MCP access is optional; it does not require or install a particular MCP server. It never submits proposals, sends messages or spends Connects without separate approval. Existing official Upwork skills remain independent.

## Plugin formats

One canonical `skills/` directory is referenced by Claude, Codex and Cursor compatibility manifests. The portable root `plugin.json` follows [Agent Plugins 1.0.0](https://agent-plugins.org/plugin-authors/manifest), with fixed `skills/` discovery. Plugin identity is `upwork-agent-connector`, version 2.0.0, avoiding the official Upwork plugin's namespace.

These packages contain instructions, not a bundled hosted MCP connection. Build and register the native connector separately. Loading this checkout in Claude can also discover its private project MCP configuration; a clean clone contains no saved credentials or connection configuration.

### Claude Code

From this repository's root, load without modifying global settings:

```bash
claude --plugin-dir .
```

Invoke `/upwork-agent-connector:upwork-project-selection` in that interactive session. Validate the package with:

```bash
claude plugin validate . --strict --json
```

### Codex, Cursor and Antigravity

Use the host's local-plugin import mechanism where supported, or copy the complete `skills/upwork-project-selection/` folder as a **real directory** into the target workspace's `.agents/skills/upwork-project-selection/`. Inspect the destination first; do not overwrite an existing skill without consent. Restart/reload the client if discovery does not refresh.

| Client | Workspace discovery / invocation | Evidence and limits |
| --- | --- | --- |
| Codex | `.agents/skills/`; `$upwork-project-selection` | [.codex-plugin manifest](../.codex-plugin/plugin.json), optional `agents/openai.yaml`; [official skills docs](https://developers.openai.com/codex/skills) |
| Cursor | `.agents/skills/` or `.cursor/skills/`; `/upwork-project-selection` | [.cursor-plugin manifest](../.cursor-plugin/plugin.json) and portable root plugin; [skills](https://cursor.com/docs/context/skills), [plugin reference](https://cursor.com/docs/plugins/building) |
| Antigravity IDE/2.0/CLI | `.agents/skills/`; `/upwork-project-selection` | Portable skill bundle; [current official skills docs](https://antigravity.google/docs/skills). Global locations differ by surface; older `.agent/skills` is backward compatibility, not the preferred workspace path. |
| Other Agent Skills clients | Their documented skill discovery/import path | Canonical standard skill bundle; do not assume a universal plugin-install command. |

Claude's actual validator passes. CI verifies manifest parity, frontmatter, catalog links and evaluation structure. Codex, Cursor and Antigravity were not installed/executed by this task: this is packaging/discovery compatibility, not a claim of runtime certification or marketplace publication. No global skill installation or permission changes occur automatically.

## Example

Ask: “Assess this job before I spend Connects. Here is the listing and my relevant experience.” The assessment marks unavailable facts as unknown, balances positive intent against risk, and gives one next step. A new client is not automatically a bad client; attractive compensation cannot override explicit unsafe contract/payment requests.

## Evaluation scope

Three reusable synthetic prompts cover a clear fit/new client, underpriced commercial unpaid work with changing requirements, and explicit payment bypass/prompt injection. They test qualitative decision behavior; structural CI cannot prove judgment quality, current marketplace facts or live account access. Human review and broader real-world evaluation remain separate from packaging validation.
