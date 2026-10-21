# Directory and registry listings (2026-10-21)

The SEO baseline ([2026-10-20](../reports/2026-10-20-seo-evidence-baseline.md))
puts the site at domain authority 2 with 3 referring domains. Listings add
referring domains and put AllSource where developers look for agent tooling:
MCP registries, skill hubs, and open-source and agent directories.

## Product facts used in every listing

Use these values verbatim so every listing matches.

```json
{
  "name": "AllSource Prime",
  "url": "https://www.all-source.xyz/prime",
  "repository": "https://github.com/all-source-os/all-source",
  "tagline": "Durable, time-travel memory for production AI agents",
  "description": "AllSource Prime gives AI agents memory that survives restarts: a knowledge graph plus vector recall, stored as an event log (WAL + Parquet) so every fact keeps its provenance and you can query what the agent knew at any point in time. It runs as an MCP server over stdio, works local-only with no account, and is Apache-2.0. A hosted AllSource tier adds cross-machine sync and team memory.",
  "category": "AI agents / developer tools / memory",
  "pricing": "Open source (Apache-2.0); hosted plans with a 14-day trial",
  "logo_url": "https://www.all-source.xyz/logo.png",
  "screenshot_url": "https://www.all-source.xyz/assets/hero-screenshot.png",
  "install": "cargo install allsource-prime"
}
```

The contact email is chosen by the founder at submission time and is not
recorded here.

## Registries and skill hubs

| Target | Route | Status |
|---|---|---|
| Official MCP Registry | `mcp-publisher publish` from `apps/prime-mcp/server.json` (MCPB bundle from the v0.26.1 release), namespace `io.github.all-source-os` | pending founder GitHub login |
| ClawHub | `clawhub skill publish skills/allsource-prime-memory --slug allsource-prime-memory` | pending founder GitHub login |
| awesome-openclaw-skills | PR adding the ClawHub listing (the list only accepts skills published on ClawHub) | after ClawHub |
| awesome-mcp-servers (punkpeye) | PR under Knowledge & Memory | [#17301](https://github.com/punkpeye/awesome-mcp-servers/pull/17301) open |
| Glama | indexes public GitHub MCP servers; claim via GitHub login | pending founder login |
| Smithery | add server from GitHub repo | pending founder login |
| mcp.so | submit form | pending founder login |
| PulseMCP | ingests the official registry | follows the registry entry |

## Directories

Taken from the 69 directories in the `listing-swarm` ClawHub skill, keeping
those whose audience includes developers or open-source and agent tooling.
Generic AI-app lists, directories marked "Bad", and directories for unrelated
categories (design, video, detection, sustainability) are left out. Captchas
and account sign-ups are completed by the founder; no captcha-solving service
is used.

| Directory | DR | Visits/month | Price | Why |
|---|---|---|---|---|
| [OpenAlternative](https://openalternative.co/submit) | 50 | 275k | free | open-source alternatives; AllSource is Apache-2.0 |
| [AI Agent Store](https://aiagentstore.ai/new-agent) | 45 | 45k | free | agent infrastructure audience |
| [MicroLaunch](https://microlaunch.net/) | 55 | 54k | free | launch directory with a developer audience |
| [SourceForge](https://sourceforge.net/create/) | 10 (listed) | 41k | free | open-source project mirror |
| [Startup Stash](https://startupstash.com/add-listing/) | 25 | 5k | free | startup tool directory |
| [Launched](https://launched.site/) | 45 | 10k | free | launch directory |
| [FoundrList](https://foundrlist.com/submit) | 48 | 30k | freemium | founder tools |
| [Simple Lister](https://simplelister.com/submit) | 30 | 9k | free | general, free backlink |
| [IndieHunt](https://indiehunt.io/submit) | 21 | 500 | free | indie launches |
| [Uneed](https://www.uneed.best/submit-a-tool) | 74 | 252k | paid (free queue) | high DR; free waiting-list option |
| [PeerPush](https://peerpush.net/submit) | 72 | 166k | paid | high DR |
| [Open Tools](https://opentools.ai/friends/launch-tool) | 69 | 3M | paid | large AI-tool audience |
| [There's An AI For That](https://theresanaiforthat.com/) | 75 | 5M | paid | largest AI directory |

Paid listings need a spending decision before submission.

## Submission log

| Date | Target | Result | Listing URL |
|---|---|---|---|
| 2026-10-22 | oh-my-openagent PR #7183 (evidence comment, not a listing) | posted; evidence ZIP offered, not attached | https://github.com/code-yeongyu/oh-my-openagent/pull/7183#issuecomment-6346440630 |
