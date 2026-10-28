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
 * digest of the release asset on GitHub. Since 2026-10-26 (release-3.0.1,
 * variant В) the table of `cli/releases/README.md` is guarded too: one row per
 * image in the tree, no row without one, each with the record's tag and digest.
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

/** The row of the README table for 2.6.3, as `cli/releases/README.md` writes it. */
const ROW_263 = `| \`Aruna_2.6.3-macos-universal.dmg\` | v2.6.3 | \`${V263}\` |`

/** A README of `cli/releases/` whose table has `rows`. */
const readme = (...rows: string[]) =>
  ['# Aruna', '', '| В репозитории | Выпуск | Сумма образа |', '|---|---|---|', ...rows, ''].join(
    '\n',
  )

/**
 * A tree with the record and the checksum file of 2.6.3, `image` as its copy,
 * and `table` as `cli/releases/README.md`.
 */
function tree(image: string | null, table: string = readme(ROW_263)): string {
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
  writeFileSync(join(root, 'cli', 'releases', 'README.md'), table)
  return root
}

/** A tree that holds on v2.6.3 – the real image of 2.6.3 – with `table` as its README. */
function heldTree(table: string): string {
  const root = tree(null, table)
  copyFileSync(
    join(ROOT, 'cli', 'releases', 'Aruna_2.6.3-macos-universal.dmg'),
    join(root, 'cli', 'releases', 'Aruna_2.6.3-macos-universal.dmg'),
  )
  return root
}

describe('the table of cli/releases/README.md', () => {
  const check = (table: string) =>
    gate('--root', heldTree(table), '--tag', 'v2.6.3', '--github-digest', V263)

  it('holds with one row per image, carrying the record', () => {
    const run = check(readme(ROW_263))
    expect(run.stderr).toBe('')
    expect(run.status).toBe(0)
  })

  it('fails on a row for an image that is not in the tree', () => {
    const run = check(
      readme(ROW_263, `| \`Aruna_3.0.0-macos-universal.dmg\` | v3.0.0 | \`${'f'.repeat(64)}\` |`),
    )
    expect(run.status).toBe(1)
    expect(run.stderr).toContain(
      'cli/releases/README.md has a row for Aruna_3.0.0-macos-universal.dmg, and there is no such image in cli/releases/',
    )
  })

  it('fails when an image in the tree has no row', () => {
    const run = check(readme())
    expect(run.status).toBe(1)
    expect(run.stderr).toContain(
      'Aruna_2.6.3-macos-universal.dmg is in cli/releases/, and cli/releases/README.md has no row for it',
    )
  })

  it('fails on a row whose checksum is not the record', () => {
    const other = '0'.repeat(64)
    const run = check(readme(`| \`Aruna_2.6.3-macos-universal.dmg\` | v2.6.3 | \`${other}\` |`))
    expect(run.status).toBe(1)
    expect(run.stderr).toContain(
      `cli/releases/README.md gives Aruna_2.6.3-macos-universal.dmg as ${other}, the reference record says ${V263}`,
    )
  })

  it('fails on a row that names another release, or no checksum', () => {
    const run = check(readme('| `Aruna_2.6.3-macos-universal.dmg` | v2.6.2 | – |'))
    expect(run.status).toBe(1)
    expect(run.stderr).toContain('names the release v2.6.2, not v2.6.3')
    expect(run.stderr).toContain('gives no SHA-256: –')
  })

  it('fails on two rows for one image', () => {
    const run = check(readme(ROW_263, ROW_263))
    expect(run.status).toBe(1)
    expect(run.stderr).toContain(
      'cli/releases/README.md has two rows for Aruna_2.6.3-macos-universal.dmg',
    )
  })

  it('fails on a row for an image the record does not know', () => {
    const run = check(
      readme(ROW_263, `| \`Aruna_9.9.9-macos-universal.dmg\` | v9.9.9 | \`${V263}\` |`),
    )
    expect(run.status).toBe(1)
    expect(run.stderr).toContain('the reference record has no v9.9.9')
  })

  it('fails when the README is missing', () => {
    const root = heldTree('')
    rmSync(join(root, 'cli', 'releases', 'README.md'))
    const run = gate('--root', root, '--tag', 'v2.6.3', '--github-digest', V263)
    expect(run.status).toBe(1)
    expect(run.stderr).toContain('cli/releases/README.md is missing')
  })
})

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
 * The references the tree keeps a copy of (part3-6, release-3.0.2): v3.0.2,
 * v2.6.3 and v2.5.11, with the digests of `Aruna-macos-universal.dmg` on
 * GitHub, read 2026-10-28; v3.0.1 left that day, replaced by v3.0.2, as
 * v3.0.0 had left on 2026-10-26.
 * Compared by name: a copy that comes or goes without its own change here
 * fails, and so does a record that names another digest.
 */
const REFERENCES: Record<string, string> = {
  'v2.5.11': 'cfd92abe41ad8b0dc9d7665144a15675e54a346a7c86fee2079838ab92f32410',
  'v2.6.3': V263,
  'v3.0.2': '8e091d885996b5f97c555fd5687d5536b3de35981aee80f1e4e7ade3fcd4f334',
}

describe('the references kept in cli/releases/', () => {
  it('are exactly v2.5.11, v2.6.3 and v3.0.2, each with its checksum file', () => {
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
