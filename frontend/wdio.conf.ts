import { finish, freePort, isolate, stopApplication, type Isolation } from './e2e/isolation'

// Where the application under test keeps what it writes. macOS takes WebKit
// data, caches and saved window state from CoreFoundation's home directory,
// which ignores `HOME` and reads `CFFIXED_USER_HOME`; the core and the
// window-state plugin take theirs from `HOME`; WebKit keeps its temporary
// files under `TMPDIR`. Until 2026-09-24 every run wrote into the real
// `~/Library` (`WebKit/aruna-desktop`, `Caches/aruna-desktop`); until
// 2026-10-11 only the first was moved, and a run of the release of 2.6.3 left
// `Application Support/com.sergeyssimonov.aruna/.window-state.json` behind
// (owner's decision of 2026-10-11, question 3 of that release). Now all three
// point into one directory of the run, removed when it ends, and the run fails
// if anything of Aruna's appeared or changed in the real `Library` (`finish`).
//
// Made in `onPrepare`, not at the top of the file: every worker loads this
// file again, and a directory made on load was left behind by each of them.
// `onPrepare` runs once, in the launcher, before the service's own
// `onPrepare` spawns the application with this object as its environment.
const appEnv: Record<string, string> = {}
let run: Isolation | undefined

// The binary the service starts, as `ps` shows it.
const application = '../target/debug/aruna-desktop'

// How long the window may take to answer once the service has its driver, and
// the sentence a run that waits that long ends with. The service polls the
// driver's `/status` before the session; the first command after it was the
// race – ECONNREFUSED in the first attempt at the acceptance of 2.6.3 – and
// it is waited for here, with a limit, rather than taken as the test's failure.
const READY_MS = 30_000

async function windowAnswers(): Promise<void> {
  const deadline = Date.now() + READY_MS
  let last: unknown
  while (Date.now() < deadline) {
    try {
      if ((await browser.execute(() => document.readyState)) === 'complete') return
    } catch (error) {
      last = error
    }
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  throw new Error(
    `Aruna E2E: the window did not answer within ${READY_MS} ms after the driver started` +
      (last ? `; last error: ${String(last)}` : ''),
  )
}

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
        application,
      },
    } as WebdriverIO.Capabilities,
  ],
  services: [['tauri', { driverProvider: 'embedded', env: appEnv }]],
  framework: 'mocha',
  reporters: ['spec'],
  logLevel: 'warn',
  mochaOpts: { ui: 'bdd', timeout: 60000 },
  onPrepare: async () => {
    run = isolate(appEnv)
    // A port nothing holds, rather than the service's fixed one: a run that
    // starts while the last one's application is still going away would
    // otherwise meet its driver.
    process.env.TAURI_WEBDRIVER_PORT = String(await freePort())
  },
  before: async () => {
    await windowAnswers()
  },
  onComplete: async () => {
    if (run) await finish(run, () => stopApplication('aruna-desktop'))
  },
}
