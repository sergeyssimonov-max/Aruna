import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build, type Plugin } from 'vite'
import { describe, expect, it } from 'vitest'
import { ARTIFACTS, SCRIPT, inventoryBuild } from '../build/inventory.ts'

/**
 * Each audit exception covers only its documented development path.
 *
 * GHSA-vfj7-8cjw-p6xm is held to its path (below); GHSA-gcqx-w48m-w2h4 to its
 * date, its versions, its path and the dist (at the end of the file).
 *
 * `pnpm-workspace.yaml` excepts GHSA-vfj7-8cjw-p6xm (braces, owner's decision
 * of 2026-10-03): braces comes only with the window tests, through
 * @wdio/mocha-framework > mocha 10 > chokidar 3, and no fixed release exists.
 * `pnpm audit` goes quiet for the identifier wherever braces turns up, so this
 * holds the exception to its reason: braces at 3.0.3 only, reached by that
 * chain only. Anything else – another dependent, another version, or no braces
 * at all – fails, and the exception is reconsidered or removed.
 */

const FRONTEND = join(dirname(fileURLToPath(import.meta.url)), '..')

const GHSA = 'GHSA-vfj7-8cjw-p6xm'

/** Fixed by raising serialize-javascript to 7.1.4 and so never to be excepted again. */
const M5MM = 'GHSA-m5mm-p8h7-m29v'

const GCQX = 'GHSA-gcqx-w48m-w2h4'
const GLOB = 'GHSA-3g3m-2fjr-6pw4'
const MINIMATCH = 'GHSA-g3hv-75wg-m249'
const EXCEPTIONS = [GHSA, GCQX, GLOB, MINIMATCH]

/** Fixed by the undici raise of 2026-10-24 and so never to be excepted again. */
const RAISED = ['GHSA-xjj6-f33q-929c', 'GHSA-4qv8-2p9j-2685', 'GHSA-f72v-6w45-q472']

const workspace = () => readFileSync(join(FRONTEND, 'pnpm-workspace.yaml'), 'utf8')
const lockfile = () => readFileSync(join(FRONTEND, 'pnpm-lock.yaml'), 'utf8')

/** Every GHSA the workspace names: `audit.ignore` is the only place one goes. */
function ignored(text: string): string[] {
  return [...text.matchAll(/GHSA-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}/g)].map((m) => m[0])
}

describe('the audit exceptions', () => {
  it('are exactly the four reviewed identifiers, through audit.ignore', () => {
    const text = workspace()
    expect(new Set(ignored(text))).toEqual(new Set(EXCEPTIONS))
    for (const id of [...RAISED, M5MM]) expect(ignored(text)).not.toContain(id)
    expect(text).not.toMatch(/auditConfig|ignoreGhsas|ignoreCves/)
  })
})

/** Each link of the excepted chain: the package, and the one allowed to depend on it. */
const CHAIN: [RegExp, RegExp][] = [
  [/^braces@3\.0\.3$/, /^chokidar@3\./],
  [/^chokidar@3\./, /^mocha@10\./],
  [/^mocha@10\./, /^@wdio\/mocha-framework@/],
]

/** `name@version` without the peer suffix pnpm adds in `snapshots`. */
function bare(key: string): string {
  const unquoted = key.replace(/^'|'$/g, '')
  const paren = unquoted.indexOf('(')
  return paren === -1 ? unquoted : unquoted.slice(0, paren)
}

/** The lines of one top-level section of the lockfile. */
function section(lock: string, name: string): string[] {
  const lines = lock.split('\n')
  const start = lines.indexOf(`${name}:`)
  if (start === -1) return []
  const end = lines.findIndex((line, i) => i > start && /^\S/.test(line))
  return lines.slice(start + 1, end === -1 ? undefined : end)
}

/** Every package the snapshots section resolves, with what it depends on. */
function snapshots(lock: string): Map<string, string[]> {
  const graph = new Map<string, string[]>()
  let current: string[] | null = null
  for (const line of section(lock, 'snapshots')) {
    const entry = /^ {2}(\S.*):$/.exec(line)
    if (entry) {
      const key = bare(entry[1])
      current = graph.get(key) ?? []
      graph.set(key, current)
      continue
    }
    const dep = /^ {6}('?[^:']+'?): (\S+)$/.exec(line)
    if (dep && current) {
      const ref = bare(dep[2])
      // An npm alias records the resolved package name in the value.
      current.push(ref.includes('@') ? ref : `${dep[1].replace(/^'|'$/g, '')}@${ref}`)
    }
  }
  return graph
}

/** The packages that depend on anything matching `target`. */
function dependents(graph: Map<string, string[]>, target: RegExp): string[] {
  return [...graph].filter(([, deps]) => deps.some((dep) => target.test(dep))).map(([pkg]) => pkg)
}

/** Why the exception no longer fits the tree, or `null` while it does. */
function outOfReason(lock: string): string | null {
  const graph = snapshots(lock)
  const braces = [...graph.keys()].filter((pkg) => pkg.startsWith('braces@'))
  if (braces.length === 0) {
    return `the lockfile resolves no braces: ${GHSA} excepts nothing, remove it`
  }
  const others = braces.filter((pkg) => pkg !== 'braces@3.0.3')
  if (others.length > 0) {
    return `braces at another version (${others.join(', ')}): ${GHSA} was granted for 3.0.3`
  }
  for (const [pkg, allowed] of CHAIN) {
    const stray = dependents(graph, pkg).filter((by) => !allowed.test(by))
    if (stray.length > 0) {
      return `${pkg.source} is reached by ${stray.join(', ')}: ${GHSA} covers only @wdio/mocha-framework > mocha 10 > chokidar 3 > braces`
    }
  }
  const direct = section(lock, 'importers').filter((line) =>
    /^ {6}'?(braces|chokidar|mocha)'?:$/.test(line),
  )
  if (direct.length > 0) {
    return `the project names ${direct.map((l) => l.trim()).join(', ')} itself`
  }
  return null
}

describe(`the audit exception ${GHSA}`, () => {
  it('is still reached by its chain alone', () => {
    expect(ignored(workspace())).toContain(GHSA)
    expect(outOfReason(lockfile())).toBeNull()
  })

  it('would fail on another path, another version, or no braces', () => {
    const chain = [
      'snapshots:',
      '',
      '  braces@3.0.3:',
      '    dependencies:',
      '      fill-range: 7.1.1',
      '',
      '  chokidar@3.6.0:',
      '    dependencies:',
      '      braces: 3.0.3',
      '',
      '  mocha@10.8.2:',
      '    dependencies:',
      '      chokidar: 3.6.0',
      '',
      "  '@wdio/mocha-framework@9.31.2(supports-color@8.1.1)':",
      '    dependencies:',
      '      mocha: 10.8.2',
      '',
    ].join('\n')
    expect(outOfReason(chain)).toBeNull()

    const micromatch = `${chain}  micromatch@4.0.8:\n    dependencies:\n      braces: 3.0.3\n`
    expect(outOfReason(micromatch)).toMatch(/is reached by micromatch@4\.0\.8/)

    const watcher = `${chain}  nodemon@3.1.0:\n    dependencies:\n      chokidar: 3.6.0\n`
    expect(outOfReason(watcher)).toMatch(/is reached by nodemon@3\.1\.0/)

    const newer = chain.replaceAll('3.0.3', '3.0.4')
    expect(outOfReason(newer)).toMatch(/another version/)

    const none = chain
      .split('\n\n')
      .filter((block) => !/braces/.test(block))
      .join('\n\n')
    expect(outOfReason(none)).toMatch(/resolves no braces/)

    const imported = `importers:\n\n  .:\n    devDependencies:\n      mocha:\n        specifier: 10.8.2\n\n${chain}`
    expect(outOfReason(imported)).toMatch(/names mocha: itself/)
  })
})

/**
 * GHSA-m5mm-p8h7-m29v (serialize-javascript below 7.1.3) was excepted from
 * 2026-10-24 to 2026-10-31 and is closed by the override at 7.1.4: no
 * serialize-javascript below the fix comes back. undici was raised the same day
 * past the three advisories in RAISED, and does not slide back either.
 */

/** The first fixed serialize-javascript. */
const M5MM_FIXED: Version = [7, 1, 3]

/** The undici each line was raised to on 2026-10-24; the line 8 one is not affected. */
const UNDICI_FLOOR: Record<number, Version> = { 6: [6, 29, 1], 7: [7, 30, 1] }

type Version = [number, number, number]

/** Every version of `name` the lockfile's packages section resolves. */
function resolved(lock: string, name: string): Version[] {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const found = new Map<string, Version>()
  for (const line of section(lock, 'packages')) {
    const m = new RegExp(`^ {2}'?${escaped}@(\\d+)\\.(\\d+)\\.(\\d+)'?:$`).exec(line)
    if (m) found.set(`${m[1]}.${m[2]}.${m[3]}`, [Number(m[1]), Number(m[2]), Number(m[3])])
  }
  return [...found.values()]
}

function atOrPast(version: Version, fix: Version): boolean {
  for (let i = 0; i < 3; i++) {
    if (version[i] !== fix[i]) return version[i] > fix[i]
  }
  return true
}

/** The serialize-javascript below the fix, or `null` when every one is at or past it. */
function serializeBelowFix(lock: string): string | null {
  const low = resolved(lock, 'serialize-javascript').filter((v) => !atOrPast(v, M5MM_FIXED))
  return low.length === 0
    ? null
    : `serialize-javascript below 7.1.3 (${M5MM}): ${low.map((v) => v.join('.')).join(', ')}`
}

/** The undici below the floor of its line, or `null` when every one is at or past it. */
function undiciBelowFloor(lock: string): string | null {
  const low = resolved(lock, 'undici').filter(
    (v) => UNDICI_FLOOR[v[0]] && !atOrPast(v, UNDICI_FLOOR[v[0]]),
  )
  return low.length === 0
    ? null
    : `undici below its raise of 2026-10-24: ${low.map((v) => v.join('.')).join(', ')}`
}

/** Every file of the window's dist, as `pnpm build` makes it, built into a directory of its own. */
async function builtDist(): Promise<Record<string, string>> {
  const out = await mkdtemp(join(tmpdir(), 'aruna-dist-'))
  try {
    await build({
      root: FRONTEND,
      configFile: join(FRONTEND, 'vite.config.ts'),
      logLevel: 'silent',
      build: { outDir: out, emptyOutDir: true },
    })
    const files: Record<string, string> = {}
    for (const entry of await readdir(out, { recursive: true, withFileTypes: true })) {
      if (!entry.isFile()) continue
      const path = join(entry.parentPath, entry.name)
      files[path.slice(out.length + 1)] = await readFile(path, 'utf8')
    }
    return files
  } finally {
    await rm(out, { recursive: true, force: true })
  }
}

/** The installed copy of each package the lockfile resolves, as `node_modules/.pnpm` lays it out. */
function installed(lock: string, pkg: string): string[] {
  return resolved(lock, pkg).map((v) =>
    join(FRONTEND, 'node_modules', '.pnpm', `${pkg}@${v.join('.')}`, 'node_modules', pkg),
  )
}

describe('the raised packages', () => {
  it('keep serialize-javascript at or past 7.1.3', () => {
    expect(resolved(lockfile(), 'serialize-javascript').length).toBeGreaterThan(0)
    expect(serializeBelowFix(lockfile())).toBeNull()
  })

  it('keep undici at or past its raise', () => {
    expect(undiciBelowFloor(lockfile())).toBeNull()
  })

  it('would fail on a serialize-javascript or an undici slid back', () => {
    const lock = (sj: string, u6 = '6.29.1', u7 = '7.30.1') =>
      `packages:\n\n  serialize-javascript@${sj}:\n  undici@${u6}:\n  undici@${u7}:\n  undici@8.12.2:\n\nsnapshots:\n`

    expect(serializeBelowFix(lock('7.1.4'))).toBeNull()
    expect(serializeBelowFix(lock('7.1.3'))).toBeNull()
    expect(serializeBelowFix(lock('7.0.5'))).toMatch(/7\.0\.5/)

    expect(undiciBelowFloor(lock('7.1.4'))).toBeNull()
    expect(undiciBelowFloor(lock('7.1.4', '6.29.1', '7.29.1'))).toMatch(/7\.29\.1/)
    expect(undiciBelowFloor(lock('7.1.4', '6.28.1', '7.30.1'))).toMatch(/6\.28\.1/)

    const withM5mm = 'audit:\n  ignore:\n    - GHSA-vfj7-8cjw-p6xm\n    - GHSA-m5mm-p8h7-m29v\n'
    expect(new Set(ignored(withM5mm))).not.toEqual(new Set(EXCEPTIONS))
    expect(ignored(withM5mm)).toContain(M5MM)
  })
})

/**
 * GHSA-gcqx-w48m-w2h4 (deepmerge 4.2.0 to 4.3.1, owner's decision of
 * 2026-10-28, release-3.0.2) is excepted for a month: no fixed deepmerge
 * exists. It comes with the build only – @sveltejs/vite-plugin-svelte calls it
 * in Node while Vite builds – and none of its code is in the dist. This keeps
 * all three true: the exception goes on its date or as soon as it is not
 * needed, nothing else depends on deepmerge, and its code does not ship.
 */

/** The exception is gone by then, whatever the lockfile says. */
const GCQX_DUE = '2026-11-30T00:00:00Z'

/** The versions the advisory names. */
const GCQX_FROM: Version = [4, 2, 0]
const GCQX_TO: Version = [4, 3, 1]

/** The one package allowed to depend on deepmerge. */
const GCQX_BY = /^@sveltejs\/vite-plugin-svelte@/

/**
 * A property deepmerge reads off its options – `options.isMergeableObject` – in
 * `dist/cjs.js`, the file its `main` names. A minifier renames identifiers but
 * keeps property names, so bundled deepmerge would carry it.
 */
const GCQX_MARKER = { pkg: 'deepmerge', file: 'dist/cjs.js', marker: 'isMergeableObject' }

/** Why GHSA-gcqx has to go, or `null` while it is still needed, in date and on its path. */
function gcqxOutOfReason(now: Date, lock: string): string | null {
  if (now.getTime() >= Date.parse(GCQX_DUE)) {
    return `срок исключения ${GCQX} истек: ${GCQX_DUE}, now ${now.toISOString()}`
  }
  const affected = resolved(lock, 'deepmerge').filter(
    (v) => atOrPast(v, GCQX_FROM) && atOrPast(GCQX_TO, v),
  )
  if (affected.length === 0) {
    return `the lockfile resolves no deepmerge in 4.2.0 to 4.3.1: снять исключение ${GCQX}`
  }
  const stray = dependents(snapshots(lock), /^deepmerge@/).filter((by) => !GCQX_BY.test(by))
  if (stray.length > 0) {
    return `deepmerge is reached by ${stray.join(', ')}: ${GCQX} covers only @sveltejs/vite-plugin-svelte`
  }
  if (section(lock, 'importers').some((line) => /^ {6}'?deepmerge'?:$/.test(line))) {
    return `the project names deepmerge itself: ${GCQX} covers a build dependency only`
  }
  return null
}

describe(`the audit exception ${GCQX}`, () => {
  it('is still needed, in date and on its path', () => {
    expect(ignored(workspace())).toContain(GCQX)
    expect(gcqxOutOfReason(new Date(), lockfile())).toBeNull()
  })

  it('covers code that does not ship', { timeout: 120_000 }, async () => {
    const files = await builtDist()
    expect(Object.keys(files)).toContain('index.html')
    const found = Object.entries(files)
      .filter(([, text]) => text.includes(GCQX_MARKER.marker))
      .map(([name]) => name)
    expect(found).toEqual([])
  })

  /** A marker that is not in its own package would find nothing anywhere. */
  it('looks for a string deepmerge really carries', () => {
    const copies = installed(lockfile(), GCQX_MARKER.pkg)
    expect(copies.length).toBeGreaterThan(0)
    for (const dir of copies) {
      expect(readFileSync(join(dir, GCQX_MARKER.file), 'utf8')).toContain(GCQX_MARKER.marker)
    }
  })

  it('would fail on the due date, once it is not needed, on another path, and when named directly', () => {
    const before = new Date('2026-11-29T23:59:59Z')
    const due = new Date(GCQX_DUE)
    const lock = (version: string, by = "'@sveltejs/vite-plugin-svelte@7.3.1(svelte@5.0.0)'") =>
      `packages:\n\n  deepmerge@${version}:\n\nsnapshots:\n\n  ${by}:\n    dependencies:\n      deepmerge: ${version}\n`

    expect(gcqxOutOfReason(before, lock('4.3.1'))).toBeNull()
    expect(gcqxOutOfReason(before, lock('4.2.0'))).toBeNull()
    expect(gcqxOutOfReason(due, lock('4.3.1'))).toMatch(/срок исключения GHSA-gcqx-w48m-w2h4 истек/)
    expect(gcqxOutOfReason(new Date('2026-10-01T00:00:00Z'), lock('4.3.1'))).toBeNull()
    expect(gcqxOutOfReason(before, lock('4.3.2'))).toMatch(/снять исключение GHSA-gcqx-w48m-w2h4/)
    expect(gcqxOutOfReason(before, lock('4.1.9'))).toMatch(/снять исключение/)
    expect(gcqxOutOfReason(before, 'packages:\n\n  undici@6.29.1:\n')).toMatch(/снять исключение/)
    expect(gcqxOutOfReason(before, lock('4.3.1', 'cosmiconfig@9.0.0'))).toMatch(
      /is reached by cosmiconfig@9\.0\.0/,
    )
    const imported = `importers:\n\n  .:\n    devDependencies:\n      deepmerge:\n        specifier: 4.3.1\n\n${lock('4.3.1')}`
    expect(gcqxOutOfReason(before, imported)).toMatch(/names deepmerge itself/)
  })
})

/**
 * Temporary development-only acceptance, not a patched dependency claim.
 * The code paths and upstream blockers are recorded in DEPENDENCY-REVIEW-3.1.0.md.
 * Any change to the affected versions, their callers or their runtime reach
 * requires another review even before the fixed expiry.
 */
const TOOLING_DUE = '2026-11-30T00:00:00Z'
const TOOLING_EDGES: [string, string[]][] = [
  ['glob@10.5.0', ['@wdio/config@9.31.9', 'archiver-utils@5.0.2']],
  ['minimatch@9.0.9', ['glob@10.5.0']],
]
const TOOLING_ROOTS = ['@wdio/cli', '@wdio/globals', '@wdio/local-runner', '@wdio/tauri-service']
type Manifest = {
  dependencies?: Record<string, string>
  devDependencies?: Record<string, string>
  optionalDependencies?: Record<string, string>
  peerDependencies?: Record<string, string>
}
const manifest = () => JSON.parse(readFileSync(join(FRONTEND, 'package.json'), 'utf8')) as Manifest

function affectedTooling(pkg: string): boolean {
  return TOOLING_EDGES.some(([target]) => pkg === target)
}

/** Whether a declared root can reach either reviewed vulnerable package. */
function reachesTooling(graph: Map<string, string[]>, root: string): boolean {
  const pending = [...graph.keys()].filter((key) => key.startsWith(root + '@'))
  const visited = new Set<string>()
  while (pending.length > 0) {
    const pkg = pending.pop()!
    if (affectedTooling(pkg)) return true
    if (visited.has(pkg)) continue
    visited.add(pkg)
    pending.push(...(graph.get(pkg) ?? []))
  }
  return false
}

function toolingOutOfReason(now: Date, lock: string, pkg: Manifest): string | null {
  if (now.getTime() >= Date.parse(TOOLING_DUE)) return 'tooling exceptions expired: ' + TOOLING_DUE
  const glob = resolved(lock, 'glob')
    .filter((v) => atOrPast(v, [9, 0, 0]) && !atOrPast(v, [14, 0, 2]))
    .map((v) => v.join('.'))
    .sort()
  const minimatch = resolved(lock, 'minimatch')
    .filter((v) => atOrPast(v, [7, 0, 0]) && !atOrPast(v, [11, 0, 1]))
    .map((v) => v.join('.'))
    .sort()
  if (JSON.stringify(glob) !== JSON.stringify(['10.5.0'])) return 'glob version changed or removed'
  if (minimatch.length !== 1 || minimatch[0] !== '9.0.9') {
    return 'minimatch version changed or removed'
  }
  const graph = snapshots(lock)
  for (const [target, expected] of TOOLING_EDGES) {
    const parents = [...graph]
      .filter(([, deps]) => deps.includes(target))
      .map(([name]) => name)
      .sort()
    if (JSON.stringify(parents) !== JSON.stringify([...expected].sort())) {
      return target + ' parent paths changed: ' + parents.join(', ')
    }
  }
  for (const group of [
    pkg.dependencies,
    pkg.devDependencies,
    pkg.optionalDependencies,
    pkg.peerDependencies,
  ]) {
    if (group && ['glob', 'minimatch'].some((name) => name in group)) {
      return 'glob/minimatch became a direct dependency'
    }
  }
  for (const group of [pkg.dependencies, pkg.optionalDependencies, pkg.peerDependencies]) {
    for (const root of Object.keys(group ?? {})) {
      if (reachesTooling(graph, root)) return 'production dependency reaches tooling: ' + root
    }
  }
  const developmentRoots = Object.keys(pkg.devDependencies ?? {}).filter((root) =>
    reachesTooling(graph, root),
  )
  if (JSON.stringify(developmentRoots.sort()) !== JSON.stringify(TOOLING_ROOTS)) {
    return 'tooling development roots changed: ' + developmentRoots.join(', ')
  }
  return null
}

/** Includes peer-qualified pnpm directories, which the unscoped marker helper does not need. */
function packageCopies(name: string, version: string): string[] {
  const store = join(FRONTEND, 'node_modules', '.pnpm')
  const prefix = name.replace('/', '+') + '@' + version
  const copies = readdirSync(store)
    .filter((entry) => entry === prefix || entry.startsWith(prefix + '_'))
    .map((entry) => join(store, entry, 'node_modules', name))
  expect(copies.length, name + '@' + version + ' is installed').toBeGreaterThan(0)
  return copies
}

/** Exact installed callers are part of the evidence, rather than inferred from package names. */
function callerSources(name: string, version: string, file: string): string[] {
  return packageCopies(name, version).map((dir) => readFileSync(join(dir, file), 'utf8'))
}

const TOOLING_MARKERS = [
  { pkg: 'glob', version: '10.5.0', file: 'dist/esm/glob.js', marker: 'windowsPathsNoEscape' },
  {
    pkg: 'minimatch',
    version: '9.0.9',
    file: 'dist/commonjs/index.js',
    marker: 'firstPhasePreProcess',
  },
]

function shippedTooling(modules: string[], files: Record<string, string>): string[] {
  return [
    ...modules.filter((id) => /[/\\]node_modules[/\\](?:glob|minimatch)[/\\]/.test(id)),
    ...Object.entries(files)
      .filter(([, text]) => TOOLING_MARKERS.some(({ marker }) => text.includes(marker)))
      .map(([name]) => name),
  ]
}

/** Inspect the two actual outgoing bundles, including every inventory artifact. */
async function toolingInBuild(kind: 'window' | 'inventory'): Promise<string[]> {
  const out = await mkdtemp(join(tmpdir(), 'aruna-tooling-' + kind + '-'))
  const modules = new Set<string>()
  const inspect: Plugin = {
    name: 'aruna-audit-shipped-modules',
    generateBundle(_options, bundle) {
      for (const chunk of Object.values(bundle)) {
        if (chunk.type === 'chunk') {
          for (const id of chunk.moduleIds) modules.add(id)
        }
      }
    },
  }
  try {
    if (kind === 'inventory') {
      const config = inventoryBuild(out)
      await build({ ...config, plugins: [...(config.plugins ?? []), inspect] })
    } else {
      await build({
        root: FRONTEND,
        configFile: join(FRONTEND, 'vite.config.ts'),
        mode: 'production',
        logLevel: 'silent',
        plugins: [inspect],
        build: { outDir: out, emptyOutDir: true },
      })
    }
    expect(modules.size, kind + ' bundle exposes its module graph').toBeGreaterThan(0)
    const files: Record<string, string> = {}
    for (const entry of await readdir(out, { recursive: true, withFileTypes: true })) {
      if (entry.isFile()) {
        const path = join(entry.parentPath, entry.name)
        files[path.slice(out.length + 1)] = await readFile(path, 'utf8')
      }
    }
    if (kind === 'inventory') {
      expect(Object.keys(files).sort()).toEqual([...ARTIFACTS].sort())
      expect(files[SCRIPT]).toBeTruthy()
    } else {
      expect(Object.keys(files)).toContain('index.html')
    }
    return shippedTooling([...modules], files)
  } finally {
    await rm(out, { recursive: true, force: true })
  }
}

describe('the temporary glob and minimatch development exceptions', () => {
  it('remain in date, at the reviewed versions and paths, with no runtime root', () => {
    expect(ignored(workspace())).toEqual(expect.arrayContaining([GLOB, MINIMATCH]))
    expect(toolingOutOfReason(new Date(), lockfile(), manifest())).toBeNull()
  })

  it('fails on expiry, version or parent drift, direct use and production reach', () => {
    const lock = lockfile()
    const pkg = manifest()
    const before = new Date('2026-11-29T23:59:59Z')
    expect(toolingOutOfReason(before, lock, pkg)).toBeNull()
    expect(toolingOutOfReason(new Date(TOOLING_DUE), lock, pkg)).toMatch(/expired/)
    expect(toolingOutOfReason(before, lock.replaceAll('glob@10.5.0', 'glob@10.5.1'), pkg)).toMatch(
      /version changed/,
    )
    expect(
      toolingOutOfReason(before, lock.replaceAll('minimatch@9.0.9', 'minimatch@11.0.1'), pkg),
    ).toMatch(/version changed or removed/)
    const oldLintCopy = lock.replace('packages:\n', 'packages:\n  minimatch@10.2.6:\n')
    expect(toolingOutOfReason(before, oldLintCopy, pkg)).toMatch(/version changed or removed/)
    const extra =
      lock +
      '\n  unexpected@1.0.0(peer@1):\n    dependencies:\n      glob-alias: glob@10.5.0\n' +
      '  unexpected@1.0.0(peer@2):\n    dependencies:\n      peer: 2.0.0\n'
    expect(toolingOutOfReason(before, extra, pkg)).toMatch(/parent paths changed/)
    expect(
      toolingOutOfReason(before, lock, {
        ...pkg,
        devDependencies: { ...pkg.devDependencies, 'archiver-utils': '5.0.2' },
      }),
    ).toMatch(/development roots changed/)
    expect(
      toolingOutOfReason(before, lock, { ...pkg, devDependencies: { glob: '10.5.0' } }),
    ).toMatch(/direct dependency/)
    for (const group of ['dependencies', 'optionalDependencies', 'peerDependencies'] as const) {
      expect(
        toolingOutOfReason(before, lock, {
          ...pkg,
          [group]: { ...pkg[group], '@wdio/cli': '9.31.9' },
        }),
      ).toMatch(/production dependency reaches tooling/)
    }
  })

  it('keeps the reviewed synchronous glob call and repository-controlled test pattern', () => {
    for (const text of callerSources('@wdio/config', '9.31.9', 'build/node/index.js')) {
      expect(text).toContain('import { sync as globSync } from "glob"')
      expect(text).toMatch(/globSync\(pattern, \{\s*cwd: rootDir,\s*matchBase: true\s*\}\)/)
    }
    for (const text of callerSources('archiver-utils', '5.0.2', 'file.js')) {
      expect(text).toContain('return glob.sync(pattern, options)')
    }
    const config = readFileSync(join(FRONTEND, 'wdio.conf.ts'), 'utf8')
    expect(config.match(/specs:\s*\[[^\]]*\]/g)).toEqual(["specs: ['./e2e/**/*.e2e.ts']"])
    expect(config).not.toMatch(/\bexclude\s*:/)
  })

  it('resolves a patched minimatch from each installed lint consumer', () => {
    const project = createRequire(join(FRONTEND, 'package.json'))
    const eslint = createRequire(project.resolve('eslint'))
    const ts = createRequire(project.resolve('typescript-eslint'))
    const fixed = resolved(lockfile(), 'minimatch')
      .filter((v) => atOrPast(v, [11, 0, 1]))
      .map((v) => v.join('.'))
    for (const caller of [
      eslint,
      createRequire(eslint.resolve('@eslint/config-array')),
      createRequire(ts.resolve('@typescript-eslint/typescript-estree')),
    ]) {
      const file = caller.resolve('minimatch/package.json')
      const installed = JSON.parse(readFileSync(file, 'utf8')) as { version: string }
      expect(fixed, file).toContain(installed.version)
    }
  })

  it('discovers every window spec through WebdriverIO', async () => {
    const pattern = './e2e/**/*.e2e.ts'
    const expected = readdirSync(join(FRONTEND, 'e2e'), { recursive: true, encoding: 'utf8' })
      .filter((file) => file.endsWith('.e2e.ts'))
      .map((file) => 'e2e/' + file)
      .sort()
    expect(expected.length).toBeGreaterThan(0)
    for (const dir of packageCopies('@wdio/config', '9.31.9')) {
      const entry = pathToFileURL(join(dir, 'build/node/index.js')).href
      const { FileSystemPathService } = (await import(entry)) as {
        FileSystemPathService: new () => { glob(pattern: string, root: string): string[] }
      }
      expect(new FileSystemPathService().glob(pattern, FRONTEND)).toEqual(expected)
    }
  })

  it('checks markers present in the real packages and detects a shipped copy', () => {
    for (const { pkg, version, file, marker } of TOOLING_MARKERS) {
      for (const text of callerSources(pkg, version, file)) expect(text).toContain(marker)
      expect(shippedTooling([], { 'injected.js': marker })).toEqual(['injected.js'])
    }
    const modules = ['/repo/node_modules/.pnpm/glob@10.5.0/node_modules/glob/dist/esm/glob.js']
    expect(shippedTooling(modules, {})).toEqual(modules)
    expect(shippedTooling(['/repo/src/app.ts'], { 'app.js': 'console.log(1)' })).toEqual([])
  })

  it('ships no glob or minimatch in either production bundle', { timeout: 120_000 }, async () => {
    expect(await toolingInBuild('window')).toEqual([])
    expect(await toolingInBuild('inventory')).toEqual([])
  })
})
