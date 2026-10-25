import { execFileSync, spawnSync } from 'node:child_process'
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, describe, expect, it } from 'vitest'

/**
 * The gate of tree cleanliness on the reference images (specification 6.8,
 * layer 1; owner's decision of 2026-10-11, variant (B)): every pair in
 * `cli/releases/` agrees with its checksum file and the reference record, and
 * the last published tag has its copy there, equal to the record and to the
 * digest of the release asset on GitHub.
 *
 * `scripts/reference-images.mjs` is run as the gate runs it, with the tag and
 * GitHub's digest given as inputs: nothing here asks git or the network, so it
 * holds the same on a tag push, where the newest tag has no entry yet.
 */

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..')
const SCRIPT = join(ROOT, 'scripts', 'reference-images.mjs')
/** The digest of `Aruna-macos-universal.dmg` of v2.6.3 on GitHub, read 2026-10-12. */
const V263 = 'e3137106d3af0354e318096d075b8bb01838e50750b13067df9df0d7250952c9'

const gate = (...args: string[]) =>
  spawnSync(process.execPath, [SCRIPT, '--check', ...args], { encoding: 'utf8' })

const temporary: string[] = []
afterEach(() => {
  for (const dir of temporary.splice(0)) rmSync(dir, { recursive: true, force: true })
})

/** A tree with the record and the checksum file of 2.6.3, and `image` as its copy. */
function tree(image: string | null): string {
  const root = mkdtempSync(join(tmpdir(), 'aruna-reference-images-'))
  temporary.push(root)
  mkdirSync(join(root, '.github'))
  mkdirSync(join(root, 'cli', 'releases'), { recursive: true })
  copyFileSync(
    join(ROOT, '.github', 'reference-release.json'),
    join(root, '.github', 'reference-release.json'),
  )
  copyFileSync(
    join(ROOT, 'cli', 'releases', 'Aruna_2.6.3-SHA256SUMS.txt'),
    join(root, 'cli', 'releases', 'Aruna_2.6.3-SHA256SUMS.txt'),
  )
  if (image !== null)
    writeFileSync(join(root, 'cli', 'releases', 'Aruna_2.6.3-macos-universal.dmg'), image)
  return root
}

describe('reference images', () => {
  it('hold on v2.6.3: the copy in the tree is the record and the release asset', () => {
    const run = gate('--root', ROOT, '--tag', 'v2.6.3', '--github-digest', `sha256:${V263}`)
    expect(run.stderr).toBe('')
    expect(run.status).toBe(0)
    expect(run.stdout).toContain('the last published tag is v2.6.3')
  })

  it('fail on a forged copy, and name it', () => {
    const run = gate('--root', tree('not the image'), '--tag', 'v2.6.3', '--github-digest', V263)
    expect(run.status).toBe(1)
    expect(run.stderr).toContain('Aruna_2.6.3-macos-universal.dmg is ')
    expect(run.stderr).toContain(`the release asset of v2.6.3 on GitHub is ${V263}`)
    expect(run.stderr).toContain(`Aruna_2.6.3-SHA256SUMS.txt says ${V263}`)
  })

  it('fail when the last published tag has no copy in the tree', () => {
    const run = gate('--root', tree(null), '--tag', 'v2.6.3', '--github-digest', V263)
    expect(run.status).toBe(1)
    expect(run.stderr).toContain('v2.6.3 has no copy of its image in cli/releases/')
    expect(run.stderr).toContain('Aruna_2.6.3-SHA256SUMS.txt has no image beside it')
  })

  it('fail when the last published tag has no entry in the record', () => {
    const run = gate('--root', ROOT, '--tag', 'v9.9.9', '--github-digest', V263)
    expect(run.status).toBe(1)
    expect(run.stderr).toContain('v9.9.9 has no entry in the reference record')
  })

  it('fail when GitHub publishes another digest than the record', () => {
    const other = '0'.repeat(64)
    const run = gate('--root', ROOT, '--tag', 'v2.6.3', '--github-digest', other)
    expect(run.status).toBe(1)
    expect(run.stderr).toContain(`the release asset of v2.6.3 on GitHub is ${other}`)
  })

  it('take the record when GitHub is not asked, and say the comparison is by hand', () => {
    const run = gate('--root', ROOT, '--tag', 'v2.6.3')
    expect(run.status).toBe(0)
    expect(run.stdout).toContain('gh release view v2.6.3 --json assets')
  })

  it('refuse a digest that is not one', () => {
    expect(gate('--root', ROOT, '--tag', 'v2.6.3', '--github-digest', 'e313').status).toBe(2)
  })

  it('order tags by version, not by text', () => {
    const last = execFileSync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        `import { lastTag } from ${JSON.stringify(SCRIPT)}; console.log(lastTag(['v2.6.3', 'v2.6.10', 'v10.0.0-rc', 'v3.0.0', 'v1.0.9', 'x']))`,
      ],
      { encoding: 'utf8' },
    )
    expect(last.trim()).toBe('v3.0.0')
  })
})

/**
 * The references the tree keeps a copy of (part3-6): v3.0.0, v2.6.3 and
 * v2.5.11, with the digests of `Aruna-macos-universal.dmg` on GitHub, read
 * 2026-10-25. Compared by name: a copy that comes or goes without its own
 * change here fails, and so does a record that names another digest.
 */
const REFERENCES: Record<string, string> = {
  'v2.5.11': 'cfd92abe41ad8b0dc9d7665144a15675e54a346a7c86fee2079838ab92f32410',
  'v2.6.3': V263,
  'v3.0.0': 'fcb9df42f00d21d65214bee26b85ea136110f2360126a5b5f30f76e548c4da6e',
}

describe('the references kept in cli/releases/', () => {
  it('are exactly v2.5.11, v2.6.3 and v3.0.0, each with its checksum file', () => {
    const names = readdirSync(join(ROOT, 'cli', 'releases'))
      .filter((name) => name.startsWith('Aruna_'))
      .sort()
    const expected = Object.keys(REFERENCES)
      .map((tag) => tag.slice(1))
      .flatMap((v) => [`Aruna_${v}-SHA256SUMS.txt`, `Aruna_${v}-macos-universal.dmg`])
      .sort()
    expect(names).toEqual(expected)
  })

  for (const [tag, digest] of Object.entries(REFERENCES)) {
    it(`hold on ${tag}: the copy is the record and the release asset`, () => {
      const run = gate('--root', ROOT, '--tag', tag, '--github-digest', `sha256:${digest}`)
      expect(run.stderr).toBe('')
      expect(run.status).toBe(0)
      expect(run.stdout).toContain(`the last published tag is ${tag}`)
    })
  }

  it('are in the record with the digests GitHub publishes', () => {
    const record = JSON.parse(
      readFileSync(join(ROOT, '.github', 'reference-release.json'), 'utf8'),
    ) as { references: { tag: string; dmg_sha256: string }[] }
    const kept = Object.fromEntries(
      record.references
        .filter(({ tag }) => tag in REFERENCES)
        .map(({ tag, dmg_sha256 }) => [tag, dmg_sha256]),
    )
    expect(kept).toEqual(REFERENCES)
  })
})
