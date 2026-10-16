import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/ui',
  use: {
    baseURL: 'http://127.0.0.1:1420',
    viewport: { width: 380, height: 620 },
    launchOptions: {
      executablePath: process.env.CHROME_BIN || '/usr/bin/google-chrome',
      args: ['--no-sandbox'],
    },
  },
  webServer: {
    command: 'npm run dev',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: true,
  },
  reporter: 'list',
});
