// Integration data module for the /install hub and per-tool install pages.
//
// One typed object per MCP client. The per-tool pages are rendered from a
// single shared template (see app/(marketing)/install/[slug]/page.tsx and
// components/install/install-page.tsx) — so ADDING A NEW TOOL is a one-object
// edit here, no new page file.
//
// Three ways to run Prime, in the order the page offers them:
//   - `remote`: the hosted MCP URL. Nothing to install; the Control Plane
//     authenticates the API key and routes to the tenant's hosted Prime (#289).
//     Only set for clients whose docs confirm a URL plus a custom Authorization
//     header — OAuth-only connectors (Claude Desktop, ChatGPT) cannot send one.
//   - `hosted`: the local stdio binary plus `--sync-to` / `--api-key`, which
//     pushes prime.* events to the tenant's Core.
//   - `local`: the same binary without those flags; memory stays on disk.
// Verified against:
//   - apps/prime-mcp/src/main.rs (the real clap CLI: --data-dir, --auto-inject,
//     --sync-to, --api-key)
//   - apps/web/src/app/(marketing)/connect/connect-client.tsx (the hosted mint
//     flow + the canonical hosted config it renders)
//   - apps/web/src/app/(marketing)/prime/page.tsx (per-client config blocks
//     already verified there: Claude Desktop, Claude Code, Cursor, OpenCode)

/** The hosted gateway Prime syncs to. Never Core directly — Core is internal-only. */
export const SYNC_TO_URL = "https://api.all-source.xyz";

/** Hosted MCP transport, served through the gateway. */
export const HOSTED_MCP_URL = `${SYNC_TO_URL}/api/v1/prime/mcp`;

/** Default on-disk memory dir used in every snippet. */
export const DATA_DIR = "~/.prime/memory";

/** crates.io install command — identical for every client. */
export const INSTALL_BINARY_CMD = "cargo install allsource-prime";

export type ConfigKind = "json" | "bash" | "toml";

export type ConfigBlock = {
  /** Short label shown above the code block, e.g. "~/.cursor/mcp.json". */
  label: string;
  /** The exact content to copy-paste. `__API_KEY__` is replaced at render. */
  content: string;
  kind: ConfigKind;
};

export type Integration = {
  /** URL slug — page lives at /install/<slug>. */
  slug: string;
  /** Display name, e.g. "Claude Desktop". */
  name: string;
  /** One-line value prop used on the hub card and page intro. */
  blurb: string;
  /**
   * Where this client stores its MCP config (file path or "no file — uses a
   * CLI command"). Shown as a hint above the paste block.
   */
  configLocation: string;
  /** Hosted MCP URL, no install. Absent when the client can't send a bearer header. */
  remote?: ConfigBlock;
  /** Hosted variant: local binary + --sync-to + --api-key. */
  hosted: ConfigBlock;
  /** Local-only variant: same binary, no account, no sync flags. */
  local: ConfigBlock;
  /**
   * Optional copy-paste prompt for an agent that can edit config files itself
   * (Claude Code, Cursor, etc.) — "set me up" in one paste.
   */
  agentPrompt?: string;
  /** Client-specific caveats / verification notes. Each item is one bullet. */
  notes: string[];
  /** Whether the client's config path is verified against vendor docs. */
  verified: boolean;
};

// `__API_KEY__` is the placeholder the install template swaps for either the
// user's freshly-minted key or a readable `<YOUR_API_KEY>` placeholder.
const KEY = "__API_KEY__";

/** Standard Claude-Desktop-shaped mcpServers JSON (Cursor, Windsurf, VS Code share it). */
function mcpServersJson(opts: { hosted: boolean; serverKey?: string }): string {
  const serverKey = opts.serverKey ?? "prime";
  const args = opts.hosted
    ? ["--data-dir", DATA_DIR, "--auto-inject", "--sync-to", SYNC_TO_URL, "--api-key", KEY]
    : ["--data-dir", DATA_DIR, "--auto-inject"];
  return JSON.stringify(
    { mcpServers: { [serverKey]: { command: "allsource-prime", args } } },
    null,
    2
  );
}

export const integrations: Integration[] = [
  {
    slug: "claude-code",
    name: "Claude Code",
    blurb:
      "Wire Prime into Claude Code with one `claude mcp add` command — no config file to edit.",
    configLocation: "No config file — registered via the `claude mcp add` CLI.",
    remote: {
      label: "Terminal — hosted, nothing to install",
      kind: "bash",
      content: `claude mcp add --transport http prime ${HOSTED_MCP_URL} \\
  --header "Authorization: Bearer ${KEY}"`,
    },
    hosted: {
      label: "Terminal — run from your project root",
      kind: "bash",
      content: `claude mcp add prime allsource-prime \\
  --data-dir ${DATA_DIR} \\
  --auto-inject \\
  --sync-to ${SYNC_TO_URL} \\
  --api-key ${KEY}`,
    },
    local: {
      label: "Terminal — local-only, no account",
      kind: "bash",
      content: `claude mcp add prime allsource-prime \\
  --data-dir ${DATA_DIR} \\
  --auto-inject`,
    },
    agentPrompt:
      "Install the AllSource Prime MCP server for me. Run `cargo install allsource-prime`, then `claude mcp add prime allsource-prime --data-dir ~/.prime/memory --auto-inject`. Confirm it registered with `claude mcp list`.",
    notes: [
      "Run `claude mcp list` to confirm the `prime` server registered.",
      "The flags after `allsource-prime` are passed through to the binary, not to `claude`.",
    ],
    verified: true,
  },
  {
    slug: "claude-desktop",
    name: "Claude Desktop",
    blurb:
      "The original Prime surface — paste one JSON block, or double-click the .dxt bundle for a no-terminal install.",
    configLocation:
      "~/Library/Application Support/Claude/claude_desktop_config.json (macOS) · ~/.config/Claude/claude_desktop_config.json (Linux) · %APPDATA%\\Claude\\claude_desktop_config.json (Windows)",
    hosted: {
      label: "claude_desktop_config.json",
      kind: "json",
      content: mcpServersJson({ hosted: true }),
    },
    local: {
      label: "claude_desktop_config.json",
      kind: "json",
      content: mcpServersJson({ hosted: false }),
    },
    notes: [
      "Easiest path: download the `.dxt` bundle from the latest GitHub release and double-click it — Claude Desktop prompts for your API key and writes the config for you.",
      "Fully quit and relaunch Claude Desktop (not just close the window) after editing the config.",
      "On macOS, Claude Desktop may need Full Disk Access for data dirs outside your home folder.",
    ],
    verified: true,
  },
  {
    slug: "cursor",
    name: "Cursor",
    blurb: "Cursor reads stdio MCP servers from the same mcpServers shape as Claude Desktop.",
    configLocation: "~/.cursor/mcp.json (global) or .cursor/mcp.json (per-project)",
    remote: {
      label: "~/.cursor/mcp.json",
      kind: "json",
      content: JSON.stringify(
        {
          mcpServers: {
            prime: { url: HOSTED_MCP_URL, headers: { Authorization: `Bearer ${KEY}` } },
          },
        },
        null,
        2
      ),
    },
    hosted: {
      label: "~/.cursor/mcp.json",
      kind: "json",
      content: mcpServersJson({ hosted: true }),
    },
    local: {
      label: "~/.cursor/mcp.json",
      kind: "json",
      content: mcpServersJson({ hosted: false }),
    },
    agentPrompt:
      'Set up the AllSource Prime MCP server in Cursor. Run `cargo install allsource-prime`, then add a `prime` entry to ~/.cursor/mcp.json with command `allsource-prime` and args ["--data-dir", "~/.prime/memory", "--auto-inject"]. Reload the window when done.',
    notes: [
      "Reload the Cursor window (or toggle the MCP server in Settings → MCP) after saving.",
      "Per-project memory: use `.cursor/mcp.json` in the repo with a project-specific `--data-dir`.",
    ],
    verified: true, // verified against https://cursor.com/docs/context/mcp on 2026-05-24
  },
  {
    slug: "windsurf",
    name: "Windsurf",
    blurb:
      "Windsurf's Cascade reads MCP servers from an mcp_config.json with the standard mcpServers envelope.",
    configLocation: "~/.codeium/windsurf/mcp_config.json",
    hosted: {
      label: "~/.codeium/windsurf/mcp_config.json",
      kind: "json",
      content: mcpServersJson({ hosted: true }),
    },
    local: {
      label: "~/.codeium/windsurf/mcp_config.json",
      kind: "json",
      content: mcpServersJson({ hosted: false }),
    },
    notes: [
      "Config path is the documented Windsurf default; open Windsurf → Settings → Cascade → MCP and click Refresh after saving, or edit the file via the 'View raw config' button if your version stores it elsewhere.",
      "Windsurf uses the same `mcpServers` JSON shape as Claude Desktop — only the file path differs.",
    ],
    verified: false, // mcp_config.json path is the documented default but varies across Windsurf versions — flagged for the reader
  },
  {
    slug: "vscode",
    name: "VS Code",
    blurb:
      "VS Code's built-in MCP support (Copilot agent mode) loads stdio servers from a workspace .vscode/mcp.json.",
    configLocation:
      ".vscode/mcp.json (per-workspace) — VS Code uses a top-level `servers` key, not `mcpServers`.",
    remote: {
      label: ".vscode/mcp.json",
      kind: "json",
      content: JSON.stringify(
        {
          servers: {
            prime: {
              type: "http",
              url: HOSTED_MCP_URL,
              headers: { Authorization: `Bearer ${KEY}` },
            },
          },
        },
        null,
        2
      ),
    },
    hosted: {
      label: ".vscode/mcp.json",
      kind: "json",
      content: JSON.stringify(
        {
          servers: {
            prime: {
              type: "stdio",
              command: "allsource-prime",
              args: [
                "--data-dir",
                DATA_DIR,
                "--auto-inject",
                "--sync-to",
                SYNC_TO_URL,
                "--api-key",
                KEY,
              ],
            },
          },
        },
        null,
        2
      ),
    },
    local: {
      label: ".vscode/mcp.json",
      kind: "json",
      content: JSON.stringify(
        {
          servers: {
            prime: {
              type: "stdio",
              command: "allsource-prime",
              args: ["--data-dir", DATA_DIR, "--auto-inject"],
            },
          },
        },
        null,
        2
      ),
    },
    notes: [
      'VS Code uses a top-level `servers` key (with `type: "stdio"`), NOT the `mcpServers` key the desktop clients use.',
      "Requires GitHub Copilot with MCP/agent mode enabled. After saving, click Start on the server in the .vscode/mcp.json gutter.",
    ],
    verified: true, // verified against https://code.visualstudio.com/docs/copilot/chat/mcp-servers
  },
  {
    slug: "chatgpt",
    name: "ChatGPT",
    blurb: "Use Prime from ChatGPT desktop via its MCP connector — the same local stdio binary.",
    configLocation:
      "ChatGPT desktop → Settings → Connectors → Add MCP server (no hand-edited file).",
    hosted: {
      label: "Connector fields — Command / Arguments",
      kind: "bash",
      content: `Command:    allsource-prime
Arguments:  --data-dir ${DATA_DIR} --auto-inject --sync-to ${SYNC_TO_URL} --api-key ${KEY}`,
    },
    local: {
      label: "Connector fields — Command / Arguments",
      kind: "bash",
      content: `Command:    allsource-prime
Arguments:  --data-dir ${DATA_DIR} --auto-inject`,
    },
    notes: [
      "MCP connector support in ChatGPT is rolling out and gated by plan/region — availability and the exact Settings path differ across builds. If you don't see a local/stdio MCP connector option, use the Claude Code or Cursor page instead.",
      "Enter the command and each argument as separate fields if the connector UI asks for an args array rather than one string.",
    ],
    verified: false, // ChatGPT's stdio MCP connector UI is in active rollout — fields/labels vary; flagged for the reader
  },
  {
    slug: "codex",
    name: "Codex",
    blurb:
      "OpenAI's Codex CLI reads MCP servers from ~/.codex/config.toml under an [mcp_servers] table.",
    configLocation: "~/.codex/config.toml",
    hosted: {
      label: "~/.codex/config.toml",
      kind: "toml",
      content: `[mcp_servers.prime]
command = "allsource-prime"
args = ["--data-dir", "${DATA_DIR}", "--auto-inject", "--sync-to", "${SYNC_TO_URL}", "--api-key", "${KEY}"]`,
    },
    local: {
      label: "~/.codex/config.toml",
      kind: "toml",
      content: `[mcp_servers.prime]
command = "allsource-prime"
args = ["--data-dir", "${DATA_DIR}", "--auto-inject"]`,
    },
    notes: [
      "Codex CLI uses TOML, not JSON, and the table is `mcp_servers` (snake_case).",
      "Run `codex mcp list` (where available) to confirm Prime registered after editing the file.",
    ],
    verified: false, // ~/.codex/config.toml + [mcp_servers] is the documented shape but Codex CLI's MCP config has changed across releases — flagged for the reader
  },
  {
    slug: "opencode",
    name: "OpenCode",
    blurb:
      'OpenCode uses its own envelope: a top-level `mcp` key, `type: "local"`, and command-as-array.',
    configLocation: "opencode.json (project root) or ~/.config/opencode/opencode.json (global)",
    remote: {
      label: "opencode.json",
      kind: "json",
      content: JSON.stringify(
        {
          $schema: "https://opencode.ai/config.json",
          mcp: {
            prime: {
              type: "remote",
              url: HOSTED_MCP_URL,
              enabled: true,
              headers: { Authorization: `Bearer ${KEY}` },
            },
          },
        },
        null,
        2
      ),
    },
    hosted: {
      label: "opencode.json",
      kind: "json",
      content: JSON.stringify(
        {
          $schema: "https://opencode.ai/config.json",
          mcp: {
            prime: {
              type: "local",
              command: [
                "allsource-prime",
                "--data-dir",
                DATA_DIR,
                "--auto-inject",
                "--sync-to",
                SYNC_TO_URL,
                "--api-key",
                KEY,
              ],
              enabled: true,
            },
          },
        },
        null,
        2
      ),
    },
    local: {
      label: "opencode.json",
      kind: "json",
      content: JSON.stringify(
        {
          $schema: "https://opencode.ai/config.json",
          mcp: {
            prime: {
              type: "local",
              command: ["allsource-prime", "--data-dir", DATA_DIR, "--auto-inject"],
              enabled: true,
            },
          },
        },
        null,
        2
      ),
    },
    notes: [
      'OpenCode\'s envelope differs from every other client: top-level `mcp`, `type: "local"`, and the command + all args go in ONE array.',
      "Set `enabled: true` so OpenCode starts the server (it defaults to enabled, but being explicit avoids surprises).",
    ],
    verified: true, // verified against https://opencode.ai/docs/mcp-servers on 2026-05-24
  },
];

export function getIntegration(slug: string): Integration | undefined {
  return integrations.find((i) => i.slug === slug);
}

/**
 * Replace the `__API_KEY__` placeholder in a snippet. When no real key is
 * available (the common case on a static page) we substitute a readable
 * `<YOUR_API_KEY>` token so the snippet is still obviously copy-pasteable.
 */
export function withApiKey(content: string, apiKey?: string): string {
  return content.replaceAll(KEY, apiKey ?? "<YOUR_API_KEY>");
}
