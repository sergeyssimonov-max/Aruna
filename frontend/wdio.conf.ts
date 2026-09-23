import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

// Where the application under test keeps its WebKit data, caches and saved
// window state. macOS takes these from CoreFoundation's home directory, which
// ignores `HOME` and reads `CFFIXED_USER_HOME`; without it every run wrote into
// the real `~/Library` (`WebKit/aruna-desktop`, `Caches/aruna-desktop`), and
// those had to be removed by hand after each run. `HOME` itself is left alone:
// the core resolves the Downloads folder through it, and the scenarios read
// what is there.
//
// The directory is made in `onPrepare`, not at the top of the file: every
// worker loads this file again, and a directory made on load was left behind
// by each of them. `onPrepare` runs once, in the launcher, before the service's
// own `onPrepare` spawns the application with this object as its environment.
const appEnv: Record<string, string> = {}

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
        application: '../target/debug/aruna-desktop',
      },
    } as WebdriverIO.Capabilities,
  ],
  services: [['tauri', { driverProvider: 'embedded', env: appEnv }]],
  framework: 'mocha',
  reporters: ['spec'],
  logLevel: 'warn',
  mochaOpts: { ui: 'bdd', timeout: 60000 },
  onPrepare: () => {
    appEnv.CFFIXED_USER_HOME = mkdtempSync(join(tmpdir(), 'aruna-e2e-home-'))
  },
  onComplete: () => {
    if (appEnv.CFFIXED_USER_HOME) rmSync(appEnv.CFFIXED_USER_HOME, { recursive: true, force: true })
  },
}
