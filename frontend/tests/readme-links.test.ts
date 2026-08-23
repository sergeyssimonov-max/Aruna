import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, normalize } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

/**
 * The documentation's local links resolve.
 *
 * This replaces `scripts/readme-links.test.mjs`, which checked the same three
 * files and was deleted on 2026-08-23 with the `scripts/` directory it lived in
 * — it had never been about the React site, it merely shared a home with it.
 *
 * It earns its place: that removal rewrote every one of these documents, and a
 * link into `src/`, `wasm/search/` or `desktop/` now points at nothing. A dead
 * link in a README is the kind of fault nobody reports and everybody hits.
 */

const ROOT = normalize(join(dirname(fileURLToPath(import.meta.url)), '..', '..'))

const DOCUMENTS = [
  'README.md',
  'PERFORMANCE.md',
  'cli/README.md',
  'docs/ARCHITECTURE.md',
  'docs/FRONTEND-CONTRACT.md',
  'docs/TESTING.md',
  'docs/FONTS.md',
]

/** Local link targets in a markdown document, with the line they sit on. */
function localLinks(markdown: string): { target: string; line: number }[] {
  const found: { target: string; line: number }[] = []
  markdown.split('\n').forEach((text, index) => {
    for (const match of text.matchAll(/\[[^\]]*\]\(([^)\s]+)\)/g)) {
      const target = match[1].split('#')[0]
      // Anchors within a document, and anything with a scheme, are somebody
      // else's to keep alive.
      if (!target || /^[a-z][a-z0-9+.-]*:/i.test(target)) continue
      found.push({ target, line: index + 1 })
    }
  })
  return found
}

describe('the documentation links to files that exist', () => {
  for (const document of DOCUMENTS) {
    it(document, () => {
      const path = join(ROOT, document)
      expect(existsSync(path), `${document} is itself missing`).toBe(true)

      const base = dirname(path)
      const dead = localLinks(readFileSync(path, 'utf8'))
        .filter(({ target }) => !existsSync(normalize(join(base, target))))
        .map(({ target, line }) => `${document}:${line} → ${target}`)

      expect(dead).toEqual([])
    })
  }
})
