import { readFileSync } from 'node:fs'
import { dirname, join, normalize } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

/**
 * The release the README calls current is the one the manifest declares.
 *
 * CI already refuses a tag that disagrees with `version` in `cli/Cargo.toml`.
 * Nothing checked the sentence a reader actually acts on: the README named
 * v2.3.0 as the current release for a day after v2.4.0 was published, and the
 * link beside it — `/releases/latest` — kept resolving correctly, which is
 * exactly why nobody noticed.
 *
 * The reference releases listed further down are deliberately out of scope:
 * they are supposed to be behind.
 *
 * The paragraph is found by its link to `/releases/latest`, not by its words:
 * the link is what a reader presses, and the words depend on the language the
 * README is written in.
 */

const ROOT = normalize(join(dirname(fileURLToPath(import.meta.url)), '..', '..'))

/** `version` from the `[package]` section of a Cargo manifest. */
function manifestVersion(relative: string): string {
  const manifest = readFileSync(join(ROOT, relative), 'utf8')
  const match = manifest.match(/^\s*version\s*=\s*"([^"]+)"/m)
  expect(match, `${relative} declares no version`).not.toBeNull()
  return match![1]
}

/** Where the README sends a reader for the release to download. */
const LATEST = 'https://github.com/sergeyssimonov-max/Aruna/releases/latest'

/** The paragraph that tells a reader which release to download. */
function currentReleaseParagraph(markdown: string): string {
  const paragraphs = markdown.split(/\n\s*\n/).filter((p) => p.includes(LATEST))
  expect(paragraphs, 'в README не ровно один абзац со ссылкой на действующий выпуск').toHaveLength(
    1,
  )
  return paragraphs[0]
}

describe('the README names the release the manifest declares', () => {
  it('README.md', () => {
    const version = manifestVersion('cli/Cargo.toml')
    const paragraph = currentReleaseParagraph(readFileSync(join(ROOT, 'README.md'), 'utf8'))

    const named = [...paragraph.matchAll(/v(\d+\.\d+\.\d+)/g)].map((match) => match[1])

    expect(named, 'действующий выпуск не назван ни одной версией').not.toHaveLength(0)
    expect([...new Set(named)]).toEqual([version])
  })
})
