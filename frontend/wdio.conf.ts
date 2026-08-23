import type { Options } from '@wdio/types'

export const config: Options.Testrunner = {
  runner: 'local',
  specs: ['./e2e/**/*.e2e.ts'],
  maxInstances: 1,
  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': {
        application: '../src-tauri/target/debug/aruna',
      },
    } as WebdriverIO.Capabilities,
  ],
  services: [['tauri', { driverProvider: 'embedded' }]],
  framework: 'mocha',
  reporters: ['spec'],
  logLevel: 'warn',
  mochaOpts: { ui: 'bdd', timeout: 60000 },
}
