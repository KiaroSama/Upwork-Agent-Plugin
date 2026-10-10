# Upwork Local Connector

A project-local connection to Upwork's hosted MCP service, with an explicitly chosen connection route.

## Language

**Upwork MCP**:
The official hosted service exposing Upwork account and marketplace capabilities.
_Avoid_: Local Upwork server

**Local connector**:
The software in this project that links the agent to the hosted Upwork MCP.
_Avoid_: Offline Upwork, self-hosted Upwork

**Proxy profile**:
A saved, named proxy destination the user can select for the local connector.
_Avoid_: Automatic fallback, proxy rotation

**Local proxy settings**:
The user's configured local intermediary endpoint, selected explicitly instead of a remote proxy profile.
_Avoid_: Upwork service port, OAuth callback port

**Worthwhile opportunity**:
A job with supported buyer readiness, proven freelancer fit and acceptable business value, not merely a job the freelancer could win.
_Avoid_: Guaranteed hire

**Blocked mode**:
The selected state in which the local connector cannot contact Upwork or its authentication services.
_Avoid_: Direct by default
