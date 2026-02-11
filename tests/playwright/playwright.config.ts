import { defineConfig, devices } from '@playwright/test';

// Set ADAPTER env var to control which adapter to test:
// ADAPTER=axum npx playwright test
// ADAPTER=cloudflare npx playwright test
// Default: axum
const adapter = process.env.ADAPTER || 'axum';

const webServerCommands: Record<string, string> = {
  axum: 'cargo run -p mocktioneer-adapter-axum',
  cloudflare: 'cd crates/mocktioneer-adapter-cloudflare && edgezero-cli serve --adapter cloudflare',
};

export default defineConfig({
  testDir: '.',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: 'http://127.0.0.1:8787',
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: adapter,
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: webServerCommands[adapter],
    cwd: '../..',
    url: 'http://127.0.0.1:8787/',
    reuseExistingServer: !process.env.CI,
    timeout: 120000,
  },
});
