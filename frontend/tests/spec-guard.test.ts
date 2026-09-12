/**
 * The decisions the specification fixed, checked against the files that hold
 * them.
 *
 * `docs/PROJECT-SPEC.ru.md` (редакция 48, 2026-09-13) settles a long list of
 * things by name — a package manager, an engine floor, a bundle identifier,
 * which Tauri plugins are registered and under which permissions, and the whole
 * mechanism by which the end-to-end contour is kept out of a release build. Its
 * §6.5 then says, in as many words, what counts as a violation.
 *
 * Until now every one of those was held by someone remembering. Several of them
 * fail *silently*: a `build.target` quietly raised produces a bundle that runs
 * on the machine that built it and nowhere else; an E2E permission moved into
 * the scanned capabilities directory breaks the ordinary build with an error
 * about a plugin nobody registered; a `tauri-plugin-log` registered under the
 * `e2e` feature panics at start-up because the wdio crates install a global
 * logger first. The specification's own deviation log (§7.1) is a list of
 * exactly these, each found the hard way.
 *
 * So this file reads the real configuration and fails when it stops saying what
 * was agreed. It asserts nothing about how the application looks or what it
 * does — only about the frame the specification drew around it.
 *
 * **It is not a copy of the specification.** Where the document explains, this
 * checks; where a value appears in two files that must agree, it is compared
 * rather than restated.
 */
import { describe, expect, it } from 'vitest'
import { readFileSync, existsSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const at = (relative: string) => fileURLToPath(new URL(relative, import.meta.url))
const text = (relative: string) => readFileSync(at(relative), 'utf8')
const json = (relative: string) => JSON.parse(text(relative))

const FRONTEND_PKG = json('../package.json')
const ROOT_PKG = json('../../package.json')
const TAURI_CONF = json('../../src-tauri/tauri.conf.json')
const CARGO = text('../../src-tauri/Cargo.toml')
const CORE_CARGO = text('../../cli/Cargo.toml')
const TAURI_LIB = text('../../src-tauri/src/lib.rs')
const VITE_CONFIG = text('../vite.config.ts')
const INDEX_HTML = text('../index.html')
const MAIN_TS = text('../src/main.ts')
const WDIO_CONF = text('../wdio.conf.ts')
const APP_SVELTE = text('../src/App.svelte')

describe('the package manager is pnpm, pinned, and alone', () => {
  it('is pinned by the root manifest, which corepack reads', () => {
    expect(ROOT_PKG.packageManager).toMatch(/^pnpm@11\.25\.0(\+|$)/)
  })

  it('leaves no trace of npm as a project manager', () => {
    for (const path of [
      '../../package-lock.json',
      '../package-lock.json',
      '../../npm-shrinkwrap.json',
      '../npm-shrinkwrap.json',
    ]) {
      expect(existsSync(at(path)), `${path} exists`).toBe(false)
    }
  })

  /**
   * A range is a decision deferred to whenever someone next installs. The
   * platform is frozen at macOS 13 and the ecosystem is leaving it — that is
   * how Playwright fell out of this stack — so every version here is a number
   * somebody chose on a day they could check it.
   */
  it('states every dependency as an exact version', () => {
    for (const [where, pkg] of [
      ['frontend', FRONTEND_PKG],
      ['root', ROOT_PKG],
    ] as const) {
      const declared = { ...(pkg.dependencies ?? {}), ...(pkg.devDependencies ?? {}) }
      for (const [name, range] of Object.entries(declared)) {
        expect(range, `${where}: ${name} is not an exact version`).toMatch(/^\d+\.\d+\.\d+/)
      }
    }
  })
})

describe('the engine floor of the window', () => {
  /**
   * The window lives in the WKWebView macOS 13 ships, which never updates
   * again. Vite 8's own default floor is higher than that, so this line is the
   * only thing between a release and a bundle that runs nowhere but the machine
   * that built it — and it fails at runtime in the field, not at build time
   * here.
   */
  it('is safari16, stated in vite.config.ts', () => {
    expect(VITE_CONFIG).toMatch(/target:\s*'safari16'/)
  })
})

describe('the Tauri application is configured as agreed', () => {
  /**
   * **The application is the product, and is named and identified as one.**
   *
   * Until 2026-09-04 the opposite was asserted here, and the reason was sound
   * while it lasted: what this project distributed was `Aruna.app`, packed by
   * `cli/build_app.sh` from the console crate, and the Tauri bundle beside it
   * was the environment the window was built in rather than a second product.
   * Two things called `Aruna.app` on a case-insensitive filesystem are one
   * file, so the shell was held to a name that could not collide with the
   * thing a reader installs.
   *
   * The release moved to the window on 2026-09-04 and the collision moved with
   * it: there is one bundle now, built by Tauri, and `cli/build_app.sh` no
   * longer produces the other one. So the shell takes the product's name and
   * the product's identifier — the `.shell` suffix existed only to keep two
   * bundles apart, and there is no second bundle to keep it apart from.
   *
   * What has not changed is that the name is a decision rather than a default:
   * the Cargo binary stays `aruna-desktop`, because `wdio.conf.ts` launches it
   * by that path, and the bundle is `Aruna` because that is what a reader sees
   * in Finder.
   */
  it('is named and identified as the product it now is', () => {
    expect(TAURI_CONF.identifier).toBe('com.sergeyssimonov.aruna')
    expect(TAURI_CONF.productName).toBe('Aruna')
    expect(TAURI_CONF.bundle.targets).toEqual(['app', 'dmg'])
  })

  /**
   * **The application says the version the core says.**
   *
   * The shell kept its own numbering while it was the environment rather than
   * the product — §3.5 said so, and said the condition for ending it: the day
   * a reader receives the shell itself. That day arrived, and two numbers for
   * one download is one too many. `cli/Cargo.toml` stays the source of truth,
   * because the release contour reads it when it checks a tag and the release
   * script reads it when it writes the bundle.
   *
   * Compared rather than restated: this test names no version at all, so a
   * release bumps two manifests and nothing else.
   */
  it('carries the version of the core it is built on', () => {
    const version = (manifest: string, which: string) => {
      const found = manifest.match(/^\s*version\s*=\s*"([^"]+)"/m)
      expect(found, `${which} declares no version`).not.toBeNull()
      return found![1]
    }
    expect(version(CARGO, 'src-tauri/Cargo.toml')).toBe(version(CORE_CARGO, 'cli/Cargo.toml'))
  })

  /**
   * **Ad hoc, and stated in the configuration rather than done to the bundle.**
   *
   * There is no Developer ID, and as of 2026-09-08 there will not be one: the
   * owner settled it — no Apple developer account, none planned — so this is a
   * boundary the project keeps rather than a gap it is closing. §6.7 records
   * what it costs a reader, and the cost is permanent. What it must not cost is
   * a second mechanism — a `codesign` call after the fact would sign whatever
   * Tauri last produced, and nothing would say when it stopped matching.
   * Tauri's own pseudo-identity is one line and travels with the bundle
   * definition.
   *
   * The minimum system version travels beside it for the same reason. Tauri's
   * default is macOS 10.13; this program targets macOS 13 and nothing older
   * (§2.1), and the bundle built on 2026-09-03 declared 10.13 because nobody
   * had said otherwise.
   */
  it('signs ad hoc and admits no system older than the one it targets', () => {
    expect(TAURI_CONF.bundle.macOS.signingIdentity).toBe('-')
    expect(TAURI_CONF.bundle.macOS.minimumSystemVersion).toBe('13.0')
  })

  /** The E2E service finds the window by the document's title, not by config. */
  it('gives the window and the document the same title', () => {
    const windowTitle = TAURI_CONF.app.windows[0].title
    const documentTitle = INDEX_HTML.match(/<title>([^<]*)<\/title>/)?.[1]
    expect(windowTitle).toBe('Aruna')
    expect(documentTitle, 'index.html and the window disagree').toBe(windowTitle)
  })

  /** Without it the E2E bridge cannot reach the Tauri API through the window. */
  it('exposes the global Tauri object', () => {
    expect(TAURI_CONF.app.withGlobalTauri).toBe(true)
  })

  it('drives the frontend through pnpm and points at its build', () => {
    expect(TAURI_CONF.build.beforeDevCommand).toContain('pnpm')
    expect(TAURI_CONF.build.beforeBuildCommand).toContain('pnpm')
    expect(TAURI_CONF.build.frontendDist).toBe('../frontend/dist')
  })
})

describe('the shell declares its dependencies exactly', () => {
  /**
   * **A range is a decision deferred to whoever next runs `cargo update`.**
   *
   * The window's manifest states every version it ships with `=`. That was
   * true of all but one line for weeks — `log = "0.4"` — and §7.3 carried the
   * pairing as a single item: pin the line, and add the check that keeps it
   * pinned. Both were done on 2026-09-08.
   *
   * The check reads the tables whose contents reach a user — `[dependencies]`
   * and `[build-dependencies]` — and stops at the first table that does not.
   * `[dev-dependencies]` is deliberately outside it: `tempfile = "3"` there
   * repeats the core's own declaration on purpose, and the core states ranges
   * throughout. Pinning the shell's copy would make the two disagree in order
   * to satisfy a rule about what ships, which that crate does not.
   *
   * A path dependency carries no version and is not a range; `aruna` is the
   * core, and it is not fetched from anywhere.
   */
  it('states every shipped dependency as an exact version', () => {
    const tables = ['[dependencies]', '[build-dependencies]']
    const lines = CARGO.split('\n')
    const declarations: string[] = []
    let inside = false
    for (const line of lines) {
      if (line.startsWith('[')) {
        inside = tables.includes(line.trim())
        continue
      }
      if (inside && /^[A-Za-z][A-Za-z0-9_-]*\s*=/.test(line)) declarations.push(line)
    }

    // Дешевая страховка от опечатки в самом тесте: строк там больше десятка, и
    // пустой список прошел бы молча.
    expect(declarations.length).toBeGreaterThan(10)

    for (const line of declarations) {
      const name = line.slice(0, line.indexOf('=')).trim()
      if (/path\s*=/.test(line)) continue
      const version = line.match(/version\s*=\s*"([^"]+)"/) ?? line.match(/=\s*"([^"]+)"/)
      expect(version, `${name} declares no version`).not.toBeNull()
      expect(version![1], `${name} is declared as a range`).toMatch(/^=\d+\.\d+\.\d+/)
    }
  })
})

describe('every registered plugin has permissions', () => {
  const DEFAULT_CAPABILITY = json('../../src-tauri/capabilities/default.json')

  /**
   * The failure this prevents is quiet in the worst way: the plugin registers,
   * the window opens, and the command fails only when a person clicks the
   * thing that calls it.
   */
  it('matches the four plugins the builder registers', () => {
    for (const [plugin, permission] of [
      ['tauri_plugin_dialog', 'dialog:default'],
      ['tauri_plugin_opener', 'opener:default'],
      ['tauri_plugin_window_state', 'window-state:default'],
      ['tauri_plugin_store', 'store:default'],
    ] as const) {
      expect(TAURI_LIB, `${plugin} is not registered`).toContain(plugin)
      expect(DEFAULT_CAPABILITY.permissions, `${permission} is not granted`).toContain(permission)
    }
    expect(DEFAULT_CAPABILITY.permissions).toContain('core:default')
  })

  /**
   * **`opener:allow-open-path` is not granted, and its absence is the fix.**
   *
   * This line said the opposite between 2026-09-02 and 2026-09-07, and it held
   * a button that never worked. The permission enables the `open_path` command
   * but leaves its path scope empty — the plugin's own words, in
   * `permissions/autogenerated/commands/open_path.toml`: "Enables the open_path
   * command without any pre-configured scope". With nothing in the scope,
   * `is_path_allowed` fails on both of its conjuncts, and every click returned
   * `Not allowed to open path …`.
   *
   * Filling the scope was not available: «Собрать в папку…» puts the inventory
   * wherever a person chose, so any scope that worked would have been `**`,
   * which is the scope's own negation. The window now calls `open_inventory`,
   * a command of the shell, and the boundary lives there and is narrower —
   * one file, the one the core names.
   */
  it('does not grant open_path, whose scope cannot be filled', () => {
    expect(DEFAULT_CAPABILITY.permissions).not.toContain('opener:allow-open-path')
    expect(TAURI_LIB).toContain('fn open_inventory')
  })

  /**
   * **The window reaches the opener through the core, not through the plugin.**
   *
   * The sibling of the check above, and the one that would catch the mistake
   * coming back from the other side: an import restored in `App.svelte` would
   * bring the empty scope back with it.
   */
  it('leaves the opener plugin to the shell', () => {
    expect(APP_SVELTE).not.toMatch(/@tauri-apps\/plugin-opener/)
    expect(FRONTEND_PKG.dependencies).not.toHaveProperty('@tauri-apps/plugin-opener')
    expect(CARGO).toContain('tauri-plugin-opener')
  })

  /** `tauri-plugin-shell` was replaced by opener on 21.08.2026 and stays out. */
  it('does not bring back the shell plugin', () => {
    expect(CARGO).not.toMatch(/tauri-plugin-shell/)
    expect(TAURI_LIB).not.toMatch(/tauri_plugin_shell/)
  })
})

describe('the end-to-end contour cannot reach a release build', () => {
  /**
   * Four independent gates, and the point of checking all four is that any one
   * of them alone would be a convention rather than a guarantee.
   */
  it('declares both wdio crates optional and only under the e2e feature', () => {
    for (const crate of ['tauri-plugin-wdio-webdriver', 'tauri-plugin-wdio']) {
      const line = CARGO.split('\n').find((l) => l.startsWith(crate))
      expect(line, `${crate} is not declared`).toBeDefined()
      expect(line, `${crate} is not optional`).toContain('optional = true')
    }
    const feature = CARGO.match(/^e2e = \[(.*)\]$/m)?.[1]
    expect(feature, 'there is no e2e feature').toBeDefined()
    expect(feature).toContain('dep:tauri-plugin-wdio-webdriver')
    expect(feature).toContain('dep:tauri-plugin-wdio')
  })

  it('registers them behind cfg, with no-ops for the build without the feature', () => {
    expect(TAURI_LIB).toMatch(/#\[cfg\(feature = "e2e"\)\]/)
    expect(TAURI_LIB).toMatch(/#\[cfg\(not\(feature = "e2e"\)\)\]/)
    expect(TAURI_LIB).toContain('noop-wdio')
  })

  /**
   * The E2E permissions live outside the directory `build.rs` scans. A capability
   * naming a plugin that only exists under a feature breaks the ordinary build
   * at compile time, which is why the file is added at runtime instead.
   */
  it('keeps the E2E capability out of the scanned directory', () => {
    const scanned = readdirSync(at('../../src-tauri/capabilities'))
    expect(scanned).toEqual(['default.json'])
    const e2e = json('../../src-tauri/capabilities-e2e/e2e.json')
    expect(e2e.permissions).toEqual(['wdio-webdriver:default', 'wdio:default'])
    expect(TAURI_LIB).toContain('add_capability')
    expect(TAURI_LIB).toContain('capabilities-e2e/e2e.json')
  })

  /** The frontend half is cut out by the bundler when the variable is unset. */
  it('imports the frontend bridge only under VITE_E2E', () => {
    expect(MAIN_TS).toMatch(/import\.meta\.env\.VITE_E2E/)
    const bridge = MAIN_TS.indexOf('@wdio/tauri-plugin')
    const gate = MAIN_TS.indexOf('VITE_E2E')
    expect(bridge, 'the bridge is not imported at all').toBeGreaterThan(-1)
    expect(gate, 'the bridge is imported outside the gate').toBeLessThan(bridge)
  })

  /**
   * Two global loggers in one process is a panic at start-up, and the wdio
   * crates install theirs first.
   */
  it('does not register the log plugin under the e2e feature', () => {
    const logLine = TAURI_LIB.indexOf('tauri_plugin_log')
    expect(logLine, 'the log plugin is gone entirely').toBeGreaterThan(-1)
    const guard = TAURI_LIB.lastIndexOf('#[cfg(not(feature = "e2e"))]', logLine)
    expect(guard, 'the log plugin is not behind the not(e2e) guard').toBeGreaterThan(-1)
  })
})

describe('the E2E runner is the one the specification chose', () => {
  it('runs WebdriverIO against the embedded driver', () => {
    expect(WDIO_CONF).toMatch(/driverProvider:\s*'embedded'/)
    expect(WDIO_CONF).toMatch(/browserName:\s*'tauri'/)
  })

  /**
   * `cargo build` sets `cfg(dev)` and the window loads `devUrl` instead of the
   * built frontend, so the DOM a scenario inspects would be empty. The build
   * has to go through the Tauri CLI, with the feature and the variable.
   */
  it('builds the application through the Tauri CLI, with the feature and the variable', () => {
    const script = FRONTEND_PKG.scripts['test:e2e']
    expect(script).toContain('VITE_E2E=1')
    expect(script).toContain('cargo tauri build')
    expect(script).toContain('--features e2e')
    expect(script).toContain('wdio run wdio.conf.ts')
  })
})
