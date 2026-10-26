import { readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { AllSourceClient } from "../../../../sdks/typescript/src/index.ts";

const state = process.env.PR366_STATE ?? `${tmpdir()}/pr366-proof`;
const secrets = JSON.parse(readFileSync(`${state}/e2e-secrets.json`, "utf8"));
const day = new Date().toISOString().slice(0, 10);

for (const [label, baseUrl] of [["gateway", "http://127.0.0.1:4901"], ["core-direct", "http://127.0.0.1:4900"]]) {
  const client = new AllSourceClient({ baseUrl, apiKey: secrets.a, retry: { maxRetries: 0 } });
  try {
    const row = await client.latest("ops_status", `probe-a:${day}`);
    console.log(JSON.stringify({ sdk: "typescript", target: label, ok: true, result: row }));
  } catch (err) {
    const e = err as { statusCode?: number; status?: number; message?: string };
    console.log(JSON.stringify({ sdk: "typescript", target: label, ok: false, status: e.statusCode ?? e.status, message: e.message }));
  }
}

const gateway = new AllSourceClient({ baseUrl: "http://127.0.0.1:4901", apiKey: secrets.a, retry: { maxRetries: 0 } });
try {
  const keys: string[] = [];
  for await (const row of gateway.latestMany("ops_status", { prefix: "probe-" })) keys.push(row.key);
  console.log(JSON.stringify({ sdk: "typescript", call: "latestMany", ok: true, keys }));
  let upserts = 0;
  for await (const item of gateway.sync("ops_status", { wait: 1 })) {
    if (item.type === "upsert") upserts++;
    if (item.type === "checkpoint") {
      console.log(JSON.stringify({ sdk: "typescript", call: "sync", ok: true, snapshot_upserts: upserts, checkpoint: item.checkpoint }));
      break;
    }
  }
} catch (err) {
  const e = err as { statusCode?: number; message?: string };
  console.log(JSON.stringify({ sdk: "typescript", call: "latestMany/sync", ok: false, status: e.statusCode, message: e.message }));
}

const raw = await fetch(`http://127.0.0.1:4901/api/v1/views/ops_status/entities/probe-a:${day}`, {
  headers: { "X-API-Key": secrets.a },
});
console.log(JSON.stringify({ raw: "X-API-Key only, gateway", status: raw.status, body: (await raw.text()).slice(0, 200) }));
const bearer = await fetch(`http://127.0.0.1:4901/api/v1/views/ops_status/entities/probe-a:${day}`, {
  headers: { Authorization: `Bearer ${secrets.a}` },
});
console.log(JSON.stringify({ raw: "Authorization Bearer, gateway", status: bearer.status }));
