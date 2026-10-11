# Project-selection guidance

## Overview

| Skill | Purpose | Output |
| --- | --- | --- |
| [upwork-project-selection](../skills/upwork-project-selection/SKILL.md) | Choose worthwhile jobs before spending time or Connects | Pursue, Clarify, Pass or Stop for payment safety; three gates, evidence and next action |

Original English synthesis of the supplied two-page Persian article; ten risks, positive buying intent and three preproposal gates. Its reported 4,281 postings/15 years of experience are unverified observations, not a statistical model or official policy. The PDF is not distributed. GPL-3.0-only, like this project.

The skill works with pasted job evidence or optional authorized MCP reads. It does not send messages, submit proposals or spend Connects without separate approval. Official Upwork skills and the native connector remain independent.

## Installation

One canonical `skills/` tree serves all clients. The portable root `plugin.json` follows [Agent Plugins](https://agent-plugins.org/plugin-authors/manifest), with OpenAI presentation under `extensions.com.openai.interface`. Claude, Codex and Cursor compatibility manifests reference the same tree. `.claude-plugin/marketplace.json` provides the install catalog, declared as `upwork-agent-plugin-marketplace`; the plugin is `upwork-agent-connector`.

Download the skills-only ZIP from [Releases](https://github.com/KiaroSama/Upwork-Agent-Plugin/releases/latest) if you do not need the native connector. Packages contain no saved proxies, account credentials or direct hosted MCP declaration.

### Install the Claude Code plugin

Register the repository marketplace:

```bash
claude plugin marketplace add KiaroSama/Upwork-Agent-Plugin
```

Install using the declared marketplace name, not the GitHub repository as the suffix:

```bash
claude plugin install upwork-agent-connector@upwork-agent-plugin-marketplace
```

Inspect the loaded component:

```bash
claude plugin details upwork-agent-connector
```

Start a new session and invoke `/upwork-agent-connector:upwork-project-selection`. For temporary local use, extract the skills ZIP and run:

```bash
claude --plugin-dir /path/to/extracted-plugin
```

Installing instructions does not build the executable or register MCP. Native registration is a separate menu operation.

### Install the Codex plugin

Add the marketplace source using the supported CLI:

```bash
codex plugin marketplace add KiaroSama/Upwork-Agent-Plugin --ref main
```

Restart the ChatGPT desktop app, open **Plugins Directory**, choose **upwork-agent-plugin-marketplace**, and install **Upwork Project Selection**. Invoke `$upwork-project-selection` in Codex. Current Codex supports this Claude-compatible catalog; root OpenAI metadata takes precedence over the `.codex-plugin` fallback. Local/plugin install availability varies by surface; CLI catalog setup is not an invented universal `codex plugin install` command.

Standalone skill copying below is also supported by Codex. See [current official plugin instructions](https://developers.openai.com/codex/plugins/build.md).

### Install the Cursor skill

Download/extract the skills-only ZIP or a platform bundle. Copy the complete `skills/upwork-project-selection/` directory into your workspace's `.agents/skills/upwork-project-selection/`. Use a real copy, not a symlink. Inspect an existing destination before replacement; keep only one copy in scanned directories.

From the extracted package on macOS/Linux:

```bash
mkdir -p /path/to/workspace/.agents/skills
```

```bash
cp -R -n ./skills/upwork-project-selection /path/to/workspace/.agents/skills/
```

On Windows, substitute your workspace path; this command refuses an existing destination:

```powershell
$destination = 'D:\YourWorkspace\.agents\skills\upwork-project-selection'; if (Test-Path -LiteralPath $destination) { throw 'Skill already exists; review it before replacement.' }; New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null; Copy-Item -LiteralPath './skills/upwork-project-selection' -Destination $destination -Recurse
```

### Install the Antigravity skill

Antigravity uses the same canonical `skills/upwork-project-selection/` payload and workspace
`.agents/skills/upwork-project-selection/` destination. Follow the collision-safe copy commands above;
keep one discovery copy. This is standalone skill installation, not native plugin installation.

Reopen the workspace and invoke `/upwork-project-selection`, or ask to use the named skill. Cursor also supports portable root and `.cursor-plugin` packages through its available plugin import interface; native workspace discovery is the documented fallback. Antigravity IDE, 2.0 and CLI use `.agents/skills`; older `.agent/skills` is compatibility only.

### Installation boundaries

| Host | Contract | Verification limit |
| --- | --- | --- |
| Claude Code | Marketplace + `.claude-plugin` | Actual validator and isolated local marketplace install/discovery |
| Codex | Portable OpenAI interface + `.codex-plugin` fallback | Official docs and structural checks; runtime not exercised |
| Cursor | Portable root / `.cursor-plugin` / `.agents/skills` | [Official format](https://cursor.com/docs/plugins/building) and structural checks; runtime not exercised |
| Antigravity | `.agents/skills` discovery | [Official docs](https://antigravity.google/docs/skills) and portable skill checks; runtime not exercised |

Other Agent Skills clients use their own documented discovery/import path. No global installation, config overwrite or permission expansion occurs automatically. Three synthetic evaluation prompts exercise good-fit/new-client, commercial unpaid work, and payment-bypass/prompt-injection decisions; they are not proof of live marketplace success or every host runtime.

Example: “Assess this job before I spend Connects. Here is the listing and my relevant experience.” The assessment marks unknown facts honestly, balances positive intent against risk and gives one bounded next action.
