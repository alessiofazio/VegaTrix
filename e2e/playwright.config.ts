import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  timeout: 120_000,
  use: {
    ...devices["Desktop Chrome"],
    baseURL: process.env.DEMO_MERCHANT_URL ?? "http://localhost:3002",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
