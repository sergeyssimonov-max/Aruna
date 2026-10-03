import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

/**
 * The one audit exception covers exactly the path it was granted for.
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
  it('is the only one, and braces is still reached by its chain alone', () => {
    const workspace = readFileSync(join(FRONTEND, 'pnpm-workspace.yaml'), 'utf8')
    const ignored = [...workspace.matchAll(/GHSA-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}/g)].map(
      (m) => m[0],
    )
    expect(new Set(ignored)).toEqual(new Set([GHSA]))
    expect(workspace).not.toMatch(/auditConfig|ignoreGhsas|ignoreCves/)

    const lock = readFileSync(join(FRONTEND, 'pnpm-lock.yaml'), 'utf8')
    expect(outOfReason(lock)).toBeNull()
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
