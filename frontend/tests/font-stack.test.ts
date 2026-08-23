import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

/**
 * The cuneiform font stack is declared twice: in `src/inventory/canonical.css`,
 * which the exported HTML carries and which holds the reasoning, and in this
 * application's `src/app.css`, which the window uses. Nothing kept the two in
 * agreement, and on 2026-08-23 the copy disappeared entirely along with the
 * directory it lived in — unnoticed, because no check read it. This is that
 * check.
 *
 * It reads the inventory's own section rather than a copy of the list, so the
 * source of truth stays in one place: changing the stack there and not here
 * fails the build, which is the whole point. Both files sat on opposite sides
 * of the language boundary until the stylesheet sources moved into `frontend/`
 * on 2026-08-23; they are neighbours now, and the mirror is still a mirror.
 */

const read = (relative: string) =>
  readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8')

const SHARED = read('../src/inventory/canonical.css')
const APP = read('../src/app.css')

/** The value of a CSS custom property, as written. */
function declaration(css: string, property: string): string {
  const start = css.indexOf(`${property}:`)
  expect(start, `${property} is not declared`).toBeGreaterThan(-1)
  const end = css.indexOf(';', start)
  expect(end, `${property} is not terminated`).toBeGreaterThan(-1)
  return css.slice(start + property.length + 1, end)
}

/** The families a font-family value names, in order, quotes stripped. */
function families(value: string): string[] {
  return value
    .split(',')
    .map((f) =>
      f
        .trim()
        .replace(/\s+/g, ' ')
        .replace(/^['"]|['"]$/g, ''),
    )
    .filter(Boolean)
}

const shared = families(declaration(SHARED, '--font-sans'))
const corpus = families(declaration(APP, '--corpus'))

describe('the cuneiform stack mirrors src/inventory/canonical.css', () => {
  it('names the same faces in the same order', () => {
    // A contiguous run, not a set: the order is a decision. Noto precedes
    // Ullikummi because both draw standard cuneiform and Noto is what the
    // 19 021 cuneiform documents render with today; swapping them would change
    // how the corpus looks.
    const at = shared.findIndex((f) => f === corpus[0])
    expect(at, `${corpus[0]} is not in --font-sans`).toBeGreaterThan(-1)
    expect(shared.slice(at, at + corpus.length)).toEqual(corpus)
  })

  it('puts them before the generic family, so a bare machine still renders', () => {
    // Only the last-resort families count. `system-ui` is not one of them: it
    // leads every stack by design and resolves to a real face.
    const generic = ['sans-serif', 'serif', 'monospace', 'cursive', 'fantasy']
    for (const stack of ['--sans', '--heading', '--mono']) {
      const named = families(declaration(APP, stack))
      const corpusAt = named.indexOf('var(--corpus)')
      const genericAt = named.findIndex((f) => generic.includes(f))
      expect(corpusAt, `${stack} does not use var(--corpus)`).toBeGreaterThan(-1)
      if (genericAt > -1) expect(corpusAt).toBeLessThan(genericAt)
    }
  })

  it('does not name Hiragino Sans GB', () => {
    // It "covers" U+E83A only in that its private-use area holds an unrelated
    // Chinese glyph at that number. Naming it would make a wrong sign the
    // official rendering of a TLHdig character.
    //
    // Only the declarations are read: both stylesheets discuss the font in a
    // comment explaining why it is absent, and a check that failed on the
    // explanation would punish writing the reason down.
    const declared = [
      declaration(SHARED, '--font-sans'),
      declaration(APP, '--corpus'),
      ...['--sans', '--heading', '--mono'].map((s) => declaration(APP, s)),
    ]
    for (const value of declared) {
      expect(families(value)).not.toContain('Hiragino Sans GB')
    }
  })
})
