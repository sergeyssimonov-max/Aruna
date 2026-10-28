import { readFileSync } from 'node:fs'
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'vite'
import { describe, expect, it } from 'vitest'

/**
 * The audit exceptions are exactly three, and each covers what it was granted for.
 *
 * GHSA-vfj7-8cjw-p6xm is held to its path (below); GHSA-m5mm-p8h7-m29v to its
 * date, its version and the dist; GHSA-gcqx-w48m-w2h4 to its date, its
 * versions, its path and the dist (both at the end of the file).
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

const M5MM = 'GHSA-m5mm-p8h7-m29v'

const GCQX = 'GHSA-gcqx-w48m-w2h4'

/** Fixed by the undici raise of 2026-10-24 and so never to be excepted again. */
const RAISED = ['GHSA-xjj6-f33q-929c', 'GHSA-4qv8-2p9j-2685', 'GHSA-f72v-6w45-q472']

const workspace = () => readFileSync(join(FRONTEND, 'pnpm-workspace.yaml'), 'utf8')
const lockfile = () => readFileSync(join(FRONTEND, 'pnpm-lock.yaml'), 'utf8')

/** Every GHSA the workspace names: `audit.ignore` is the only place one goes. */
function ignored(text: string): string[] {
  return [...text.matchAll(/GHSA-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}/g)].map((m) => m[0])
}

describe('the audit exceptions', () => {
  it('are exactly GHSA-vfj7, GHSA-m5mm and GHSA-gcqx, through audit.ignore', () => {
    const text = workspace()
    expect(new Set(ignored(text))).toEqual(new Set([GHSA, M5MM, GCQX]))
    for (const id of RAISED) expect(ignored(text)).not.toContain(id)
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
      current = []
      graph.set(bare(entry[1]), current)
      continue
    }
    const dep = /^ {6}('?[^:']+'?): (\S+)$/.exec(line)
    if (dep && current) {
      current.push(bare(`${dep[1].replace(/^'|'$/g, '')}@${dep[2]}`))
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
 * GHSA-m5mm-p8h7-m29v (serialize-javascript below 7.1.3, owner's decision of
 * 2026-10-24) is excepted for a week: the fix, 7.1.3, was replaced by 7.1.4
 * within a day, and neither is old enough to take (specification 5.1). The
 * package comes with the window tests – @wdio/mocha-framework > mocha 10 – and
 * is held at 7.0.5 by an override. The same day undici was raised past the
 * three advisories it carried, and this keeps both facts true: the exception
 * goes on its date or as soon as it is not needed, undici does not slide back,
 * and neither package's code is in what ships.
 */

/** The exception is gone by then, whatever the lockfile says. */
const M5MM_DUE = '2026-10-31T00:00:00Z'

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

/** Why GHSA-m5mm has to go, or `null` while it is still needed and in date. */
function m5mmOutOfReason(now: Date, lock: string): string | null {
  if (now.getTime() >= Date.parse(M5MM_DUE)) {
    return `срок исключения ${M5MM} истек: ${M5MM_DUE}, now ${now.toISOString()}`
  }
  const versions = resolved(lock, 'serialize-javascript')
  if (versions.length === 0) {
    return `the lockfile resolves no serialize-javascript: снять исключение ${M5MM}`
  }
  if (versions.every((v) => atOrPast(v, M5MM_FIXED))) {
    return `every serialize-javascript is at or past 7.1.3 (${versions.map((v) => v.join('.')).join(', ')}): снять исключение ${M5MM}`
  }
  return null
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

/**
 * A string each package carries verbatim and no other code is likely to: the
 * error code undici's `lib/core/errors.js` defines, and the placeholder
 * serialize-javascript's `index.js` builds its output around. A minifier
 * renames identifiers but keeps string literals, so if either package were
 * bundled its string would be in the output.
 */
const MARKERS: { pkg: string; file: string; marker: string }[] = [
  { pkg: 'undici', file: 'lib/core/errors.js', marker: 'UND_ERR_CONNECT_TIMEOUT' },
  { pkg: 'serialize-javascript', file: 'index.js', marker: '@__(F|R|D|M|S|A|U|I|B|L)-' },
]

/** The markers found in a set of built files: none, while nothing of either package ships. */
function shipped(files: Record<string, string>): string[] {
  const found: string[] = []
  for (const [name, text] of Object.entries(files)) {
    for (const { pkg, marker } of MARKERS) {
      if (text.includes(marker)) found.push(`${pkg} in ${name}`)
    }
  }
  return found
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

describe(`the audit exception ${M5MM}`, () => {
  it('is still needed and in date', () => {
    expect(ignored(workspace())).toContain(M5MM)
    expect(m5mmOutOfReason(new Date(), lockfile())).toBeNull()
  })

  it('stands beside an undici that stays raised', () => {
    expect(undiciBelowFloor(lockfile())).toBeNull()
  })

  it('covers code that does not ship', { timeout: 120_000 }, async () => {
    const files = await builtDist()
    expect(Object.keys(files)).toContain('index.html')
    expect(shipped(files)).toEqual([])
  })

  /** A marker that is not in its own package would find nothing anywhere. */
  it('looks for strings each package really carries', () => {
    const lock = lockfile()
    for (const { pkg, file, marker } of MARKERS) {
      const copies = installed(lock, pkg)
      expect(copies.length, pkg).toBeGreaterThan(0)
      for (const dir of copies) {
        expect(readFileSync(join(dir, file), 'utf8'), `${dir}/${file}`).toContain(marker)
      }
    }
  })

  it('would fail on the due date, once it is not needed, on an undici slid back, and on a bundled package', () => {
    const before = new Date('2026-10-30T23:59:59Z')
    const due = new Date(M5MM_DUE)
    const lock = (sj: string, u6 = '6.29.1', u7 = '7.30.1') =>
      `packages:\n\n  serialize-javascript@${sj}:\n  undici@${u6}:\n  undici@${u7}:\n  undici@8.12.2:\n\nsnapshots:\n`

    expect(m5mmOutOfReason(before, lock('7.0.5'))).toBeNull()
    expect(m5mmOutOfReason(due, lock('7.0.5'))).toMatch(/срок исключения GHSA-m5mm-p8h7-m29v истек/)
    expect(m5mmOutOfReason(before, lock('7.1.3'))).toMatch(/снять исключение GHSA-m5mm-p8h7-m29v/)
    expect(m5mmOutOfReason(before, lock('7.1.4'))).toMatch(/снять исключение GHSA-m5mm-p8h7-m29v/)
    expect(m5mmOutOfReason(before, 'packages:\n\n  undici@6.29.1:\n')).toMatch(/снять исключение/)

    expect(undiciBelowFloor(lock('7.0.5'))).toBeNull()
    expect(undiciBelowFloor(lock('7.0.5', '6.29.1', '7.29.1'))).toMatch(/7\.29\.1/)
    expect(undiciBelowFloor(lock('7.0.5', '6.28.1', '7.30.1'))).toMatch(/6\.28\.1/)

    expect(shipped({ 'assets/index.js': 'const e="UND_ERR_CONNECT_TIMEOUT"' })).toEqual([
      'undici in assets/index.js',
    ])
    expect(shipped({ 'assets/a.js': "'\"@__(F|R|D|M|S|A|U|I|B|L)-'" })).toEqual([
      'serialize-javascript in assets/a.js',
    ])

    const vfj7Only = 'audit:\n  ignore:\n    - GHSA-vfj7-8cjw-p6xm\n'
    expect(new Set(ignored(vfj7Only))).not.toEqual(new Set([GHSA, M5MM, GCQX]))
    expect(ignored(`${vfj7Only}    - ${RAISED[0]}\n`)).toContain(RAISED[0])
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
