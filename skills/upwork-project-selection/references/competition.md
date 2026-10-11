# Competition and client hiring evidence

Read current `find_jobs` tool help before using these fields; the hosted service can change.
For a chosen job, use `find_jobs` action `get`. Do not fetch full details for every search row.

| Evidence | Meaning and limit |
| --- | --- |
| `proposal_count` | Exact count only when the service/account plan exposes it. |
| `proposals_tier` | A range such as 20 to 50 or 50+; never convert to an exact count or midpoint. |
| `activityStat.jobActivity.invitesSent` | Invitations sent for this posting, not clients or proposals. |
| `totalInvitedToInterview` | Posting interview funnel count as defined by the live service, not necessarily currently interviewing. |
| `totalHired`, `totalOffered` | Hires/offers for this posting; check hiring progress before recommending application. |
| `client_record` | Live lifetime hiring/spend/feedback/hours/posting/hire-rate/open-job record. |
| `client.total_posted_jobs` | Posts, not hires. Search may omit hire totals; read job details. |
| `client_work_history`, `client_feedback`, `client_open_jobs` | Limited samples, never complete totals. |
| `applied` | This freelancer already applied; do not recommend another application. |
| `connects_cost`, `connects_balance` | Cost of this application versus wallet balance; missing is unknown, not free. |

Report two separate sections: **This posting's competition/activity** and **Client history**.
For each material figure include source, observation time, exact/range/unknown and applicable scope.
Prefer live `client_record` when it differs from search snapshots. Do not derive exact rates from incomplete
samples, equate messages with customers, or predict hiring probability from applicant count.
A low client rating concerns freelancer feedback about the client; it is not a measure of client success.
A new client with no record is not automatically unsafe. Payment verification is not a guarantee.

Use competition alongside the existing three gates, not a replacement for proven fit or acceptable economics.
Many proposals may still be worthwhile; few proposals do not guarantee hiring. Ask only for missing facts
that change the decision. Respect ignored filters, pagination and account-plan restrictions.

If the tool is unavailable, request pasted job evidence or report the metric unavailable. Never switch to
an unrouted/direct connector, scrape private account data or invent a new MCP action to fill a gap.
