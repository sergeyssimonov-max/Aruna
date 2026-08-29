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
 */

const ROOT = normalize(join(dirname(fileURLToPath(import.meta.url)), '..', '..'))

/** `version` from the `[package]` section of a Cargo manifest. */
function manifestVersion(relative: string): string {
  const manifest = readFileSync(join(ROOT, relative), 'utf8')
  const match = manifest.match(/^\s*version\s*=\s*"([^"]+)"/m)
  expect(match, `${relative} declares no version`).not.toBeNull()
  return match![1]
}

/** The paragraph that tells a reader which release to download. */
function currentReleaseParagraph(markdown: string): string {
  const paragraphs = markdown.split(/\n\s*\n/).filter((p) => p.includes('current release'))
  expect(paragraphs, 'README no longer names a current release').toHaveLength(1)
  return paragraphs[0]
}

describe('the README names the release the manifest declares', () => {
  it('README.md', () => {
    const version = manifestVersion('cli/Cargo.toml')
    const paragraph = currentReleaseParagraph(readFileSync(join(ROOT, 'README.md'), 'utf8'))

    const named = [...paragraph.matchAll(/v(\d+\.\d+\.\d+)/g)].map((match) => match[1])

    expect(named, 'the current release is named by no version at all').not.toHaveLength(0)
    expect([...new Set(named)]).toEqual([version])
  })
})
