import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  testMatch: '**/*.spec.ts',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 30_000,
  outputDir: './artifacts/results',
  reporter: [['list'], ['json', { outputFile: './artifacts/results.json' }]],
  use: {
    baseURL: 'http://127.0.0.1:1420',
    viewport: { width: 1440, height: 960 },
    reducedMotion: 'reduce',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  projects: [{
    name: 'chromium',
    use: {
      ...devices['Desktop Chrome'],
      viewport: { width: 1440, height: 960 },
      ...(process.env.PLAYWRIGHT_CHANNEL ? { channel: process.env.PLAYWRIGHT_CHANNEL } : {}),
    },
  }],
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    cwd: '..',
  },
});
