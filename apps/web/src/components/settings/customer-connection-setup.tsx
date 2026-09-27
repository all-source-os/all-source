"use client";

import { Button } from "@allsource/ui";
import { useState } from "react";

export type IssuedConnection = {
  id: string;
  token: string;
  expires_at: number;
  binding: Record<string, string>;
  consent_version?: "review-metadata-v1" | "review-evidence-v2";
};

const clipboardReaders = {
  mac: "pbpaste",
  wayland: "wl-paste --no-newline",
  x11: "xclip -selection clipboard -o",
};

function quote(value: string) {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

function absolutePath(value: string) {
  return (
    value.startsWith("/") &&
    value.length <= 2048 &&
    Array.from(value).every(
      (character) => character.charCodeAt(0) >= 32 && character.charCodeAt(0) !== 127
    )
  );
}

export function CustomerConnectionSetup({ issued }: { issued: IssuedConnection }) {
  const [release, setRelease] = useState("");
  const [path, setPath] = useState("");
  const [platform, setPlatform] = useState<keyof typeof clipboardReaders>("mac");
  const [notice, setNotice] = useState("");
  const ready = absolutePath(release) && absolutePath(path) && path.endsWith(".json");
  const root = release.replace(/\/+$/, "");
  const install = `${clipboardReaders[platform]} | ${quote(`${root}/bin/allsource-customer-connection`)} --install ${quote(path)}`;
  const host = {
    command: `${root}/bin/mcp_server_elixir`,
    args: ["start"],
    env: {
      ALLSOURCE_CUSTOMER_REVIEW: "true",
      ...(issued.consent_version === "review-evidence-v2"
        ? { ALLSOURCE_CUSTOMER_EVIDENCE_REVIEW: "true" }
        : {}),
      CUSTOMER_REVIEW_CONNECTION_FILE: path,
    },
  };
  const register = `claude mcp add-json --scope local allsource_review ${quote(JSON.stringify(host))}`;

  async function copy(value: string, message: string) {
    try {
      await navigator.clipboard.writeText(value);
      setNotice(message);
    } catch {
      setNotice("Clipboard access failed. Allow clipboard access and try again.");
    }
  }

  function copyConnection() {
    try {
      const audience = new URL(issued.binding.resource ?? "");
      if (
        audience.protocol !== "https:" ||
        audience.username ||
        audience.password ||
        audience.search ||
        audience.hash
      ) {
        throw new Error("Invalid connection");
      }
      void copy(
        JSON.stringify({
          version: 1,
          url: audience.origin,
          token: issued.token,
          binding: issued.binding,
        }),
        "Connection data copied. Return to your waiting terminal and press Enter. Never paste this data into chat."
      );
    } catch {
      setNotice("Connection configuration is unavailable. Revoke this connection and try again.");
    }
  }

  return (
    <div className="space-y-4 text-base leading-relaxed">
      <p>
        Use the AllSource MCP release for your computer. Its installer saves this connection for
        your OS account. Other programs running as you can still read it.
      </p>
      <label className="block space-y-2">
        <span>MCP release directory</span>
        <input
          className="w-full rounded-md border bg-background p-3 text-base"
          value={release}
          placeholder="/Users/you/allsource-mcp"
          onChange={(event) => setRelease(event.target.value)}
        />
      </label>
      <label className="block space-y-2">
        <span>New private connection file</span>
        <input
          className="w-full rounded-md border bg-background p-3 text-base"
          value={path}
          placeholder="/Users/you/.allsource-review/connection.json"
          onChange={(event) => setPath(event.target.value)}
        />
      </label>
      <p className="text-muted-foreground">
        Use full paths without symlinks. The installer creates the final directory privately if
        needed and refuses to overwrite an existing file. Choose a new filename when reconnecting.
      </p>
      <label className="block space-y-2">
        <span>Clipboard tool on your computer</span>
        <select
          className="w-full rounded-md border bg-background p-3 text-base"
          value={platform}
          onChange={(event) => setPlatform(event.target.value as keyof typeof clipboardReaders)}
        >
          <option value="mac">macOS — pbpaste</option>
          <option value="wayland">Linux Wayland — wl-paste</option>
          <option value="x11">Linux X11 — xclip</option>
        </select>
      </label>
      <ol className="list-decimal space-y-5 pl-6">
        <li className="space-y-2">
          <p>Copy this command into your terminal. Wait before pressing Enter.</p>
          {ready && (
            <pre className="overflow-x-auto rounded-md bg-muted p-3 text-base">
              <code>{install}</code>
            </pre>
          )}
          <Button
            variant="outline"
            disabled={!ready}
            onClick={() =>
              void copy(
                install,
                "Install command copied. Paste it into your terminal, then return here without running it."
              )
            }
          >
            Copy install command
          </Button>
        </li>
        <li className="space-y-2">
          <p>
            Copy the private connection data, then return to your waiting terminal and press Enter.
          </p>
          <Button variant="outline" disabled={!ready} onClick={copyConnection}>
            Copy connection data
          </Button>
          <p className="text-muted-foreground">
            Success exits silently. If installation fails, check both paths and file permissions. Do
            not paste the connection data into chat, a shell command or a shared file.
          </p>
        </li>
        <li className="space-y-2">
          <p>
            After installation succeeds, register the server in your current Claude Code project.
          </p>
          {ready && (
            <pre className="overflow-x-auto rounded-md bg-muted p-3 text-base">
              <code>{register}</code>
            </pre>
          )}
          <Button
            variant="outline"
            disabled={!ready}
            onClick={() =>
              void copy(
                register,
                "Claude Code command copied. This replaces the private data on your clipboard."
              )
            }
          >
            Copy Claude Code command
          </Button>
          <p>
            Open Claude Code and check <code>/mcp</code>. This connection checks access and
            validates proposals
            {issued.consent_version === "review-evidence-v2"
              ? ", prepares comparisons from sources you share, and reads pending reviews"
              : ""}
            . It cannot approve or execute changes.
          </p>
        </li>
      </ol>
      {notice && <p role="status">{notice}</p>}
    </div>
  );
}
