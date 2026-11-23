import path from "node:path";
import { defineConfig } from "@playwright/test";

// The web app must be built with NEXT_PUBLIC_POSTHOG_KEY set; without it analytics never initialises.
export default defineConfig({
  testDir: "./tests/analytics",
  timeout: 30_000,
  workers: 1,
  retries: 0,
  reporter: "list",
  use: { baseURL: "http://127.0.0.1:4319" },
  webServer: {
    command: "bun run start --port 4319",
    cwd: path.resolve(import.meta.dirname, "../../apps/web"),
    url: "http://127.0.0.1:4319",
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
