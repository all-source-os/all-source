/** Upper bound for a public form route's call to the Control Plane. */
export const UPSTREAM_TIMEOUT_MS = 8000;

/**
 * The visitor's address as Fly's edge saw it. Fly-Client-IP is set by the proxy
 * and cannot be supplied by the client; X-Forwarded-For entries left of the
 * proxy's own append are client-controlled, so only the rightmost one is used.
 */
export function clientIpForUpstream(headers: Headers): string | undefined {
  const flyClientIp = headers.get("fly-client-ip")?.trim();
  if (flyClientIp) return flyClientIp;
  const forwardedFor = headers.get("x-forwarded-for");
  if (!forwardedFor) return undefined;
  const last = forwardedFor.split(",").at(-1)?.trim();
  return last || undefined;
}

/**
 * Reads at most `maxBytes` of the body. Returns null when the declared or
 * actual size exceeds the cap, without buffering the rest.
 */
export async function readCappedText(request: Request, maxBytes: number): Promise<string | null> {
  const declared = Number(request.headers.get("content-length"));
  if (Number.isFinite(declared) && declared > maxBytes) return null;
  if (!request.body) return "";

  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > maxBytes) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const body = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder().decode(body);
}
