import { NextResponse } from "next/server";
import { historyHeaders } from "./event-history-http";

export function escapeHtml(value: string) {
  const entities: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  };
  return value.replace(/[&<>"']/g, (char) => entities[char] ?? "");
}

export function historyPage(title: string, content: string, status = 200, callbackOrigin?: string) {
  const safeTitle = escapeHtml(title);
  return new NextResponse(
    `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${safeTitle} | AllSource</title>
<style>
  :root { color-scheme: light dark; font-family: Inter, system-ui, sans-serif; line-height: 1.55; }
  body { margin: 0; background: Canvas; color: CanvasText; }
  main { max-width: 39rem; margin: clamp(2rem, 8vw, 7rem) auto; padding: 0 1.5rem; }
  header { border-bottom: 1px solid GrayText; padding-bottom: 1rem; font-weight: 650; }
  h1 { font-size: clamp(2rem, 6vw, 3rem); line-height: 1.08; letter-spacing: -.045em; }
  p { margin: 1rem 0; }
  fieldset { border: 0; margin: 1.5rem 0; padding: 0; }
  label { display: block; margin: .65rem 0; }
  button {
    font: inherit; background: CanvasText; color: Canvas; cursor: pointer;
    padding: .55rem 1rem; border: 1px solid CanvasText; border-radius: .4rem;
  }
  button.secondary { background: Canvas; color: CanvasText; margin-left: .4rem; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: .5rem 1.5rem; }
  dd { margin: 0; overflow-wrap: anywhere; }
  small { display: block; margin-top: 2rem; }
</style>
</head><body><main><header>AllSource Event History</header><h1>${safeTitle}</h1>${content}</main></body></html>`,
    {
      status,
      headers: {
        ...historyHeaders,
        // Native form POSTs need an Origin header. Send only the origin, never the private citation path.
        "Referrer-Policy": "strict-origin",
        "Content-Type": "text/html; charset=utf-8",
        "Content-Security-Policy":
          "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'" +
          (callbackOrigin ? " " + new URL(callbackOrigin).origin : ""),
      },
    }
  );
}

export function historyUnavailable(path?: string) {
  return historyPage(
    "Event History unavailable",
    "<p>AllSource could not confirm access or retrieve this record.</p>" +
      (path
        ? `<a href="${escapeHtml(path)}">Check again</a>`
        : '<a href="/dashboard/settings/connections">Agent connections</a>'),
    503
  );
}

export function signInAgain() {
  return historyPage(
    "Sign in to continue",
    '<p>This workspace requires a verified AllSource account.</p><form method="post"><button name="decision" value="signin">Sign in to AllSource</button></form>',
    401
  );
}
