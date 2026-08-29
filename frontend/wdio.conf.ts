// `WebdriverIO.Config`, not `Options.Testrunner`: the second is the shape of a
// standalone remote session and has no `capabilities` key, so this file was
// annotated with a type it does not satisfy. Nothing said so, because until
// `tsconfig.e2e.json` existed no project included this file.
export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./e2e/**/*.e2e.ts'],
  maxInstances: 1,
  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': {
        application: '../src-tauri/target/debug/aruna-desktop',
      },
    } as WebdriverIO.Capabilities,
  ],
  services: [['tauri', { driverProvider: 'embedded' }]],
  framework: 'mocha',
  reporters: ['spec'],
  logLevel: 'warn',
  mochaOpts: { ui: 'bdd', timeout: 60000 },
}
