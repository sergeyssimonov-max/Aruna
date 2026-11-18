import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, realpathSync } from 'node:fs'
import { createRequire } from 'node:module'
import { basename, dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const FRONTEND = join(dirname(fileURLToPath(import.meta.url)), '..')
const localRequire = createRequire(import.meta.url)
const viteRequire = createRequire(localRequire.resolve('vite/package.json'))
let wdioRequire = createRequire(
  realpathSync(join(FRONTEND, 'node_modules/@wdio/mocha-framework/package.json')),
)
for (const dependency of ['mocha', 'chokidar', 'anymatch']) {
  wdioRequire = createRequire(wdioRequire.resolve(dependency))
}

type Options = Record<string, unknown>
interface Picomatch {
  (pattern: string | string[], options?: Options): (input: string) => boolean
  makeRe(pattern: string, options?: Options): RegExp
  parse(pattern: string, options?: Options): unknown
}

const copies = [
  {
    version: '2.3.2',
    entry: wdioRequire.resolve('picomatch'),
    patchHash: '4c7c63adfd1904005423c87ae6485be0e1dc05550b122b519d4a274418d00358',
    codeHash: '99c3473c83b6355ef2001eb2bfdc017f501ba1aec8f7c3df9427f4bd3c26df06',
  },
  {
    version: '4.0.7',
    entry: viteRequire.resolve('picomatch'),
    patchHash: '7578969f1aa97cbc2ad20da4385805c021829e8aa39ef082946658160e61f3a2',
    codeHash: '87b66ebd17cb39140502dd8675760f8b728f2807f16caac287286f2bab21b906',
  },
]

// Fixed reviewed digests, not expectations derived from the current patch.
// An upstream update or local patch change requires a new explicit review.
function verifyInstalledCode(dir: string) {
  const manifest = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')) as {
    name: string
    version: string
  }
  expect(manifest.name).toBe('picomatch')
  const reviewed = copies.find((copy) => copy.version === manifest.version)
  if (!reviewed) throw new Error(`Unreviewed picomatch version: ${manifest.version}`)
  expect(realpathSync(dir)).toContain(
    `picomatch@${reviewed.version}_patch_hash=${reviewed.patchHash}`,
  )
  const files = [
    'package.json',
    'index.js',
    'lib/constants.js',
    'lib/parse.js',
    'lib/picomatch.js',
    'lib/scan.js',
    'lib/utils.js',
  ]
  if (manifest.version === '4.0.7') files.push('posix.js')
  const digests = files.sort().map((file) => [
    file,
    createHash('sha256')
      .update(readFileSync(join(dir, file)))
      .digest('hex'),
  ])
  expect(createHash('sha256').update(JSON.stringify(digests)).digest('hex')).toBe(reviewed.codeHash)
}

it('covers every active installed picomatch copy, including aliases and additional parents', () => {
  const result = spawnSync('pnpm', ['list', '--depth', 'Infinity', '--json'], {
    cwd: FRONTEND,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    timeout: 10_000,
    killSignal: 'SIGKILL',
  })
  expect(result.error, result.stderr).toBeUndefined()
  expect(result.status, result.stdout + result.stderr).toBe(0)
  const visited = new Set<string>()
  const found = new Set<string>()
  function visit(value: unknown) {
    if (value === null || typeof value !== 'object') return
    const item = value as Record<string, unknown>
    if (typeof item.path === 'string' && !visited.has(item.path)) {
      visited.add(item.path)
      const manifestPath = join(item.path, 'package.json')
      // pnpm also lists uninstalled optional binaries for other platforms.
      // A missing picomatch itself is never accepted as a verified copy.
      if (existsSync(manifestPath)) {
        const dir = realpathSync(item.path)
        const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as { name: string }
        if (manifest.name === 'picomatch') {
          verifyInstalledCode(dir)
          found.add(dir)
        }
      } else if (item.from === 'picomatch' || basename(item.path) === 'picomatch') {
        throw new Error(`Missing installed picomatch: ${item.path}`)
      }
    }
    for (const child of Object.values(item)) visit(child)
  }
  visit(JSON.parse(result.stdout))
  expect(found).toEqual(new Set(copies.map(({ entry }) => realpathSync(dirname(entry)))))
}, 15_000)

const CODE = 'ERR_PICOMATCH_UNSAFE_EXTGLOB'
const attacks = [
  '+(*a)',
  '*(*)b',
  '+(*)b',
  '*(*a)',
  '+(**a)',
  '+(a*b)',
  '+([ab]*a)',
  '+(@(*a))',
  '+(x|*a)',
  '+(*a|x)',
  'pre/+(*a).txt',
  '+(*(a))b',
  '!+(*a)',
  '!(+(*a))',
  '*(+(*a))',
  './+(*a)',
  '+(a+(b))',
]
const modes: Options[] = [
  {},
  { windows: true },
  { bash: true },
  { capture: true },
  { dot: true },
  { nocase: true },
  { contains: true },
  { regex: true },
  { regex: false },
  { fastpaths: false },
  { maxExtglobRecursion: false },
  { maxExtglobRecursion: 100 },
  { unescape: true },
]

describe.each(copies)('picomatch $version local ReDoS repair', ({ version, entry, patchHash }) => {
  const pm = localRequire(entry) as Picomatch

  it('uses the recorded patch through the real Vite or WDIO dependency chain', () => {
    const patch = readFileSync(join(FRONTEND, 'patches', `picomatch@${version}.patch`))
    const hash = createHash('sha256').update(patch).digest('hex')
    expect(hash).toBe(patchHash)
    verifyInstalledCode(dirname(entry))
    expect(entry).toContain(`picomatch@${version}_patch_hash=${hash}`)
    const lock = readFileSync(join(FRONTEND, 'pnpm-lock.yaml'), 'utf8')
    expect(lock).toContain(`picomatch@${version}: ${hash}`)
    const packages = lock.split('\npackages:\n')[1].split('\nsnapshots:\n')[0]
    expect(new Set([...packages.matchAll(/^ {2}picomatch@([^:]+):$/gm)].map((m) => m[1]))).toEqual(
      new Set(copies.map((copy) => copy.version)),
    )
    const snapshots = lock.split('\nsnapshots:\n')[1]
    expect(snapshots).toBeDefined()
    const references = [...snapshots.matchAll(/^ {6}picomatch: (.+)$/gm)].map((match) => match[1])
    expect(references.length).toBeGreaterThan(0)
    expect(new Set(references)).toEqual(
      new Set(copies.map((copy) => `${copy.version}(patch_hash=${copy.patchHash})`)),
    )
    expect(references.filter((ref) => ref.startsWith(version))).not.toHaveLength(0)
    for (const ref of references.filter((ref) => ref.startsWith(version))) {
      expect(ref).toBe(`${version}(patch_hash=${hash})`)
    }
    // A local source repair must not masquerade as an upstream security release
    // or create a blanket ignore. audit.sh passes a one-invocation exception
    // only after this whole file succeeds, as authorized on 2026-11-18.
    expect(JSON.parse(readFileSync(join(dirname(entry), 'package.json'), 'utf8')).version).toBe(
      version,
    )
    expect(readFileSync(join(FRONTEND, 'pnpm-workspace.yaml'), 'utf8')).not.toContain(
      'GHSA-fc5q-39xc-vjpf',
    )
  })

  it('rejects nested unbounded repetition through every string compilation entry', () => {
    for (const options of modes) {
      for (const glob of attacks) {
        for (const compile of [
          () => pm(glob, options),
          () => pm.makeRe(glob, options),
          () => pm.parse(glob, options),
          () => pm(['*.txt', glob], options),
          () => pm('**/*', { ...options, ignore: glob }),
        ]) {
          expect(compile, JSON.stringify({ glob, options })).toThrowError(
            expect.objectContaining({ name: 'SyntaxError', code: CODE }),
          )
        }
      }
    }
    const parse = localRequire(join(dirname(entry), 'lib/parse.js')) as (pattern: string) => unknown
    expect(() => parse('+(*a)')).toThrowError(expect.objectContaining({ code: CODE }))
    if (version === '4.0.7') {
      const posix = localRequire(join(dirname(entry), 'posix.js')) as Picomatch
      expect(() => posix('+(*a)')).toThrowError(expect.objectContaining({ code: CODE }))
    }
  })

  it('does not hang on the original short-input trigger', () => {
    const source = `
      const pm = require(${JSON.stringify(entry)});
      for (const pattern of ['+(*a)', '*(*)b']) {
        try { pm(pattern)('a'.repeat(42) + '!'); process.exit(2); }
        catch (error) { if (error.code !== ${JSON.stringify(CODE)}) throw error; }
      }
    `
    // Kill the child if a removed/bypassed patch restores catastrophic matching.
    const result = spawnSync(process.execPath, ['-e', source], {
      encoding: 'utf8',
      timeout: 1500,
      killSignal: 'SIGKILL',
    })
    expect(result.error, result.stderr).toBeUndefined()
    expect(result.status, result.stderr).toBe(0)
  })

  it('preserves ordinary paths, finite extglobs, negation and literal metacharacters', () => {
    const cases: [string, string, boolean][] = [
      ['src/**/*.ts', 'src/lib/a.ts', true],
      ['src/**/*.ts', 'src/lib/a.js', false],
      ['**/*.test.{ts,js}', 'tests/a.test.ts', true],
      ['**/*.{js,ts,svelte}', 'src/App.svelte', true],
      ['**/!(*.d).{ts,tsx}', 'src/a.ts', true],
      ['**/!(*.d).{ts,tsx}', 'src/a.d.ts', false],
      ['file.@(js|ts)', 'file.ts', true],
      ['+(ab)', 'abab', true],
      ['*(ab)', 'ab', true],
      ['+([ab])', 'aba', true],
      ['+(\\*)', '**', true],
      ['+("*")', '**', true],
      ['+([*])', '**', true],
      ['+(\\+)', '++', true],
      ['папка/**/*.xml', 'папка/текст.xml', true],
    ]
    for (const [glob, input, expected] of cases) {
      expect(pm(glob)(input), JSON.stringify({ glob, input })).toBe(expected)
    }
  })
})
