import path from "node:path";
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/seo",
  timeout: 30_000,
  workers: 1,
  retries: 0,
  reporter: "list",
  use: { baseURL: "http://127.0.0.1:4318" },
  webServer: {
    command: "bun run start --port 4318",
    cwd: path.resolve(import.meta.dirname, "../../apps/web"),
    url: "http://127.0.0.1:4318",
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
