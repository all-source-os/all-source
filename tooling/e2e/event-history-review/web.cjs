const { createServer } = require("node:http");
const { createRequire } = require("node:module");
const path = require("node:path");

async function serve() {
  const dir = process.env.HISTORY_WEB_APP_DIR;
  const challenge = process.env.HISTORY_WEB_CHALLENGE;
  if (!dir || !path.isAbsolute(dir) || !/^[a-f0-9]{64}$/.test(challenge || "")) {
    throw new Error("Missing private reviewer configuration");
  }
  const next = createRequire(path.join(dir, "package.json"))("next");
  const app = next({ dev: false, dir, hostname: "127.0.0.1", port: 14900 });
  await app.prepare();
  const handle = app.getRequestHandler();
  const server = createServer((request, response) => {
    if (request.method === "GET" && request.url === "/.fixture/ready") {
      response.writeHead(200, { "content-type": "text/plain", "cache-control": "no-store" });
      response.end(challenge);
      return;
    }
    void handle(request, response);
  });
  server.listen(14900, "127.0.0.1");
  process.once("SIGTERM", () => server.close(() => void app.close()));
}

serve().catch(() => {
  process.stderr.write(
    "Synthetic web startup failed. Check the production build and local port.\n"
  );
  process.exitCode = 1;
});
