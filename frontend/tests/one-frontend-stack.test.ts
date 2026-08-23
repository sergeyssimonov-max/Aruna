/**
 * One frontend technology, checked rather than remembered.
 *
 * **React is not to reappear anywhere.** It was the public inventory site — a
 * React 19 application on TanStack Start — and it was retired on 2026-08-23
 * together with everything that existed to serve it. Nothing replaced it in
 * kind: the desktop window is Svelte, and what the program *writes* is a static
 * document whose script and stylesheet are built from `src/inventory/`. So the
 * rule is not "React was swapped for Svelte here and there"; it is that this
 * repository has one frontend stack and React is not part of it.
 *
 * **SvelteKit is not part of it either.** `docs/FRONTEND-CONTRACT.md` has held
 * that from the start, and until 2026-08-23 the check could not be written
 * honestly: the React application's own `src/routes/` was TanStack Router and
 * legitimate, so `src/routes` was not evidence of anything. That application is
 * gone and the ambiguity with it.
 *
 * Both halves were true when this was written. The point of the file is that
 * they stay true without anyone remembering to look.
 */
import { describe, expect, it } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { join } from 'node:path'

const at = (relative: string) => fileURLToPath(new URL(relative, import.meta.url))

/** Every file a person writes here, whatever its extension. */
function sources(dir: string, found: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name === 'coverage') {
      continue
    }
    const path = join(dir, entry.name)
    if (entry.isDirectory()) sources(path, found)
    else found.push(path)
  }
  return found
}

const manifest = (relative: string) =>
  JSON.parse(readFileSync(at(relative), 'utf8')) as {
    scripts?: Record<string, string>
    dependencies?: Record<string, string>
    devDependencies?: Record<string, string>
  }

const MANIFESTS = { frontend: manifest('../package.json'), root: manifest('../../package.json') }

const declared = (m: (typeof MANIFESTS)['root']) => [
  ...Object.keys(m.dependencies ?? {}),
  ...Object.keys(m.devDependencies ?? {}),
]

/** What pnpm actually put on disk, which is the only ground truth about a lock file. */
const installed = readdirSync(at('../node_modules/.pnpm'))

const TREE = [at('../src'), at('../tests'), at('../build'), at('../e2e')].flatMap((dir) =>
  sources(dir),
)

/**
 * The packages that would mean React is back. Exact names, not a prefix: this
 * has to tell `react` from `react-is`, which is a different thing entirely and
 * is allowed — see below.
 */
const REACT = new Set([
  'react',
  'react-dom',
  'preact',
  '@types/react',
  '@types/react-dom',
  '@vitejs/plugin-react',
  '@vitejs/plugin-react-swc',
])

/** The package name inside a `.pnpm` directory entry: `@types+node@22.20.1` → `@types/node`. */
function packageName(entry: string): string {
  const at = entry.lastIndexOf('@')
  return (at > 0 ? entry.slice(0, at) : entry).replace('+', '/')
}

describe('React is gone and does not come back', () => {
  it('is in neither manifest', () => {
    for (const [where, m] of Object.entries(MANIFESTS)) {
      expect(
        declared(m).filter((name) => REACT.has(name)),
        `${where}/package.json`,
      ).toEqual([])
    }
  })

  /**
   * `react-is` is the one thing with the name on it that may be here: a value
   * serialiser `pretty-format` uses to print a React element if it is ever
   * handed one, reached through `@testing-library/dom` and `jest-diff`. It has
   * no React in it and never runs outside a test reporter — so it is named here
   * rather than left to look like an oversight.
   */
  it('is not installed, transitively or otherwise', () => {
    const lock = readFileSync(at('../pnpm-lock.yaml'), 'utf8')
    for (const name of REACT) {
      expect(lock, `${name} is in the lock file`).not.toMatch(
        new RegExp(`^ +'?${name.replace('/', '\\/')}'?@`, 'm'),
      )
    }
    // And what is actually on disk, which a lock file does not promise: an
    // orphan left in the store links to nothing and imports nowhere, but it is
    // still a copy of React in this repository, and `pnpm prune` is a shorter
    // conversation than working out why it is there.
    const reacty = installed.map(packageName).filter((name) => REACT.has(name))
    expect(reacty, 'a React package reached node_modules/.pnpm').toEqual([])
  })

  it('is imported by no source file, and no file is JSX', () => {
    for (const file of TREE) {
      expect(file, 'JSX has no place in a Svelte project').not.toMatch(/\.[jt]sx$/)
      const text = readFileSync(file, 'utf8')
      expect(text, `${file} imports React`).not.toMatch(
        /from\s+['"](react|react-dom|preact)(\/|['"])|require\(['"]react/,
      )
    }
  })

  /** What the Rust binary compiles in and writes into every exported document. */
  it('is absent from the artifacts the crate carries', () => {
    const generated = at('../../cli/src/generated')
    for (const name of readdirSync(generated)) {
      const text = readFileSync(join(generated, name), 'utf8')
      expect(text, `${name} mentions React`).not.toMatch(/\breact\b/i)
    }
  })
})

describe('the stack is Svelte, and Svelte without SvelteKit', () => {
  it('renders the window from Svelte components', () => {
    const svelte = TREE.filter((file) => file.endsWith('.svelte'))
    expect(svelte.length, 'the window is not built from Svelte components').toBeGreaterThan(0)
    expect(declared(MANIFESTS.frontend)).toContain('svelte')
    expect(declared(MANIFESTS.frontend)).toContain('@sveltejs/vite-plugin-svelte')
  })

  it('has no SvelteKit anywhere', () => {
    expect(declared(MANIFESTS.frontend).filter((n) => n.startsWith('@sveltejs/kit'))).toEqual([])
    expect(installed.filter((n) => n.startsWith('@sveltejs/kit@'))).toEqual([])
    expect(installed.filter((n) => /^@sveltejs\/adapter-/.test(n))).toEqual([])
    for (const script of Object.values(MANIFESTS.frontend.scripts ?? {})) {
      expect(script, 'a script calls svelte-kit').not.toMatch(/\bsvelte-kit\b/)
    }
    for (const file of TREE) {
      expect(file, 'src/routes is SvelteKit routing').not.toMatch(/\/src\/routes\//)
      expect(readFileSync(file, 'utf8'), `${file} imports SvelteKit`).not.toMatch(
        /from\s+['"]@sveltejs\/kit/,
      )
    }
  })
})
