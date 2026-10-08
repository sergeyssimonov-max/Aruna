import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

/**
 * The notice file of third-party licences covers the lock files, exactly.
 *
 * `THIRD-PARTY-NOTICES.md` (owner's decision of 2026-10-02) is written by
 * `scripts/third-party-notices.mjs` from the local registry and node_modules,
 * which CI does not have. This reads only what the repository carries – the
 * three lock files and the notice file – so it holds the coverage wherever it
 * runs: every package of `Cargo.lock` (resolved with every feature, as
 * `cargo metadata --all-features` resolves it), of `frontend/pnpm-lock.yaml`
 * and of `pnpm-lock.yaml` is named once, nothing else is, and each has a
 * licence text or stands under "Requires review". A lock file that moves
 * without the file being written again fails here.
 */

const FRONTEND = join(dirname(fileURLToPath(import.meta.url)), '..')
const ROOT = join(FRONTEND, '..')
const read = (...parts: string[]) => readFileSync(join(...parts), 'utf8')

/** `name version` of every registry crate in Cargo.lock. */
function crates(): string[] {
  return read(ROOT, 'Cargo.lock')
    .split('[[package]]')
    .slice(1)
    .filter((block) => /^source = "registry\+/m.test(block))
    .map((block) => {
      const field = (k: string) => new RegExp(`^${k} = "([^"]*)"`, 'm').exec(block)?.[1]
      return `${field('name') ?? ''} ${field('version') ?? ''}`
    })
}

/** `name version` of the `packages:` section of a pnpm lock file. */
function npm(lock: string): string[] {
  const lines = lock.split('\n')
  const start = lines.indexOf('packages:')
  const out: string[] = []
  for (const line of lines.slice(start + 1)) {
    if (/^\S/.test(line)) break
    const m = /^ {2}'?([^' ][^']*?)'?:$/.exec(line)
    if (!m?.[1]) continue
    const at = m[1].lastIndexOf('@')
    out.push(`${m[1].slice(0, at)} ${m[1].slice(at + 1)}`)
  }
  return out
}

interface Row {
  key: string
  texts: string
}

/** The rows of the notice file's table: `from: name version`, and its texts. */
function rows(notices: string): Row[] {
  const table = notices.split('## Packages')[1]?.split('\n## ')[0] ?? ''
  return table
    .split('\n')
    .filter((l) => l.startsWith('| ') && !l.startsWith('| Package'))
    .map((l) => {
      const cells = l
        .slice(2, -2)
        .split(/(?<!\\) \| /)
        .map((c) => c.replaceAll('\\|', '|'))
      return {
        key: `${cells[2] ?? ''}: ${cells[0] ?? ''} ${cells[1] ?? ''}`,
        texts: cells[4] ?? '',
      }
    })
}

describe('third-party notices', () => {
  const notices = read(ROOT, 'THIRD-PARTY-NOTICES.md')
  const listed = rows(notices)

  it('name every package of the three lock files and nothing else', () => {
    const want = [
      ...crates().map((k) => `crate: ${k}`),
      ...npm(read(FRONTEND, 'pnpm-lock.yaml')).map((k) => `npm: ${k}`),
      ...npm(read(ROOT, 'pnpm-lock.yaml')).map((k) => `npm (root): ${k}`),
    ].sort()
    const got = listed.map((r) => r.key).sort()
    expect(want.length).toBeGreaterThan(1000)
    expect(got).toEqual(want)
  })

  it('give each a licence text or list it under "Requires review"', () => {
    const texts = new Set([...notices.matchAll(/^### (T\d+)$/gm)].map((m) => m[1]))
    const review = notices.split('## Requires review')[1]?.split('\n## ')[0] ?? ''
    for (const row of listed) {
      if (row.texts === 'requires review') {
        const [, rest] = row.key.split(': ')
        expect(review, row.key).toContain(`- ${rest ?? ''} (`)
      } else {
        for (const t of row.texts.split(', ')) expect(texts.has(t), `${row.key} → ${t}`).toBe(true)
      }
    }
  })

  it('ship inside the application', () => {
    const conf = JSON.parse(read(ROOT, 'src-tauri', 'tauri.conf.json')) as {
      bundle: { resources: Record<string, string> }
    }
    expect(conf.bundle.resources['../THIRD-PARTY-NOTICES.md']).toBe('THIRD-PARTY-NOTICES.md')
  })
})
