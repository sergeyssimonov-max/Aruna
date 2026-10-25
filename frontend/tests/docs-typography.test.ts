/**
 * The Russian documents keep the project's typography.
 *
 * Since 2026-10-20 the README files and the release pages are Russian (owner's
 * decision of that day), and CLAUDE.md sets one typography for every Russian
 * text: the en dash only, `е` for `ё`, no comma before a dash, a non-breaking
 * space before `%` and after `№`. Nothing held the rule but the person writing,
 * and a translation is exactly where an em dash or a `ё` slips in unseen.
 *
 * Code is not prose: a fenced block and inline code are carried byte for byte,
 * so both are taken out before the rules are asked – inline code is replaced
 * by a placeholder rather than dropped, or a comma before it would read as a
 * comma before the next dash. The one em dash left in these files is the value
 * `—` an empty field gets, and it is inline code.
 *
 * A file with no Cyrillic at all fails too: an English file would otherwise
 * pass every rule here in silence.
 *
 * The release pages are files in `.github/release-notes/`, one per tag, named
 * `vX.Y.Z.md`, and every reference release in `.github/reference-release.json`
 * has one: the file is what `gh release edit --notes-file` puts on the page.
 */
import { describe, expect, it } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const at = (relative: string) => fileURLToPath(new URL(relative, import.meta.url))

const NOTES = '../../.github/release-notes/'
const NOTE_FILES = readdirSync(at(NOTES)).filter((name) => name.endsWith('.md'))

const DOCUMENTS = [
  'README.md',
  'cli/README.md',
  'cli/releases/README.md',
  'frontend/README.md',
  ...NOTE_FILES.map((name) => `.github/release-notes/${name}`),
]

/** Each line of a document with its code taken out, numbered from one. */
function prose(relative: string): { line: number; text: string }[] {
  const lines = readFileSync(at(`../../${relative}`), 'utf8').split('\n')
  const out: { line: number; text: string }[] = []
  let fenced = false
  lines.forEach((text, index) => {
    if (text.trimStart().startsWith('```')) {
      fenced = !fenced
      return
    }
    if (!fenced) out.push({ line: index + 1, text: text.replace(/`[^`]*`/g, 'код') })
  })
  return out
}

/** The lines of `relative` where `wrong` finds something, as `file:line`. */
function offending(relative: string, wrong: (text: string) => boolean): string[] {
  return prose(relative)
    .filter(({ text }) => wrong(text))
    .map(({ line, text }) => `${relative}:${line}: ${text.trim()}`)
}

for (const document of DOCUMENTS) {
  describe(document, () => {
    it('has no em dash outside code', () => {
      expect(offending(document, (text) => text.includes('—'))).toEqual([])
    })

    it('has no ё outside the word Богазкёй', () => {
      expect(offending(document, (text) => /[ёЁ]/.test(text.replaceAll('Богазкёй', '')))).toEqual(
        [],
      )
    })

    it('has no comma before a dash', () => {
      expect(offending(document, (text) => /,\s*–/.test(text))).toEqual([])
    })

    it('has a non-breaking space before % after a digit and after №', () => {
      expect(offending(document, (text) => /\d[ \t]%|№[ \t]/.test(text))).toEqual([])
    })

    it('is written in Russian', () => {
      expect(
        prose(document).some(({ text }) => /[А-Яа-я]/.test(text)),
        `${document} has no Cyrillic outside code`,
      ).toBe(true)
    })
  })
}

describe('.github/release-notes', () => {
  it('names every file after its tag, vX.Y.Z.md', () => {
    expect(NOTE_FILES.filter((name) => !/^v\d+\.\d+\.\d+\.md$/.test(name))).toEqual([])
  })

  it('has a page for every reference release', () => {
    const record = JSON.parse(readFileSync(at('../../.github/reference-release.json'), 'utf8')) as {
      references: { tag: string }[]
    }
    const missing = record.references
      .map(({ tag }) => `${tag}.md`)
      .filter((name) => !NOTE_FILES.includes(name))
    expect(missing).toEqual([])
  })

  /**
   * A page goes on the release as it is, so a label of the draft – `{DMG_SHA256}`,
   * `{REFERENCE_DECISION – …}` – must not reach it. Code is not taken out here:
   * the labels stood in backticks.
   */
  it('has no unfilled {LABEL} left from a draft', () => {
    const labels = NOTE_FILES.flatMap((name) =>
      readFileSync(at(`${NOTES}${name}`), 'utf8')
        .split('\n')
        .flatMap((text, index) =>
          /\{[A-Z][A-Z0-9_]*/.test(text) ? [`${name}:${index + 1}: ${text.trim()}`] : [],
        ),
    )
    expect(labels).toEqual([])
  })
})
