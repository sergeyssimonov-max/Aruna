import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

const SCRIPT = fileURLToPath(new URL('../../scripts/third-party-notices.mjs', import.meta.url))
const SENTINEL = 'Previously reviewed licence texts must survive a missing install.\n'

/** Exercise the real generator in a small checkout, with no registry or build
 * dependencies on the host. A missing install must not become a successful,
 * much smaller notice file. Off-platform packages may still require review. */
describe('the third-party notice generator', { timeout: 20_000 }, () => {
  let root: string

  beforeEach(() => {
    root = realpathSync(mkdtempSync(join(tmpdir(), 'aruna notices ')))
    mkdirSync(join(root, 'scripts'))
    copyFileSync(SCRIPT, join(root, 'scripts/third-party-notices.mjs'))
    writeFileSync(join(root, 'scripts/third-party-license-order.json'), '[]\n')
    mkdirSync(join(root, 'scripts/third-party-sources'))
    writeFileSync(join(root, 'scripts/third-party-sources/npm-reviewed.json'), '[]\n')
    writeFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), SENTINEL)
    writeFileSync(
      join(root, 'Cargo.lock'),
      '[[package]]\nname = "example-crate"\nversion = "1.0.0"\nsource = "registry+test"\n',
    )
    const crate = join(root, 'cargo/registry/src/test/example-crate-1.0.0')
    mkdirSync(crate, { recursive: true })
    writeFileSync(
      join(crate, 'Cargo.toml'),
      '[package]\nname = "example-crate"\nversion = "1.0.0"\nlicense = "MIT"\n',
    )
    writeFileSync(join(crate, 'LICENSE'), 'Example crate licence text.\n')
    for (const dir of ['.', 'frontend']) {
      const installed = join(root, dir, 'node_modules/.pnpm/example@1.0.0/node_modules/example')
      mkdirSync(installed, { recursive: true })
      writeFileSync(
        join(root, dir, 'pnpm-lock.yaml'),
        'packages:\n  example@1.0.0:\n  off-platform@1.0.0:\n',
      )
      writeFileSync(
        join(installed, 'package.json'),
        JSON.stringify({ name: 'example', version: '1.0.0', license: 'MIT' }),
      )
      writeFileSync(join(installed, 'LICENSE'), 'Example npm licence text.\n')
    }
    mkdirSync(join(root, 'bin'))
    writeFileSync(
      join(root, 'bin/cargo'),
      '#!/bin/sh\nprintf "called\\n" > "$ARUNA_TEST_CARGO_CALLED"\n' +
        'printf \'%s\\n\' \'{"packages":[{"name":"example-crate","version":"1.0.0","source":"registry+test"}]}\'\n',
      { mode: 0o755 },
    )
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  function run(args: string[] = []) {
    return spawnSync(process.execPath, [join(root, 'scripts/third-party-notices.mjs'), ...args], {
      encoding: 'utf8',
      timeout: 15_000,
      env: {
        ...process.env,
        PATH: `${join(root, 'bin')}:/usr/bin:/bin`,
        CARGO_HOME: join(root, 'cargo'),
        ARUNA_TEST_CARGO_CALLED: join(root, 'cargo-called'),
      },
    })
  }

  function reviewedNpm() {
    const bytes = 'Reviewed upstream licence.\n'
    const source = join(root, 'scripts/third-party-sources')
    writeFileSync(join(source, 'reviewed-LICENSE'), bytes)
    writeFileSync(
      join(source, 'npm-reviewed.json'),
      JSON.stringify([
        {
          name: 'off-platform',
          version: '1.0.0',
          license: 'MIT',
          files: [
            {
              file: 'reviewed-LICENSE',
              sha256: createHash('sha256').update(bytes).digest('hex'),
              source: 'https://example.test/immutable/LICENSE',
            },
          ],
        },
      ]),
    )
    return source
  }

  it('uses an offline exact-version npm reference and preserves its bytes', () => {
    reviewedNpm()
    expect(run().status).toBe(0)
    const notice = readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')
    expect(notice).toContain('| off-platform | 1.0.0 | npm | MIT | T')
    expect(notice).toContain('Reviewed upstream licence.\n')
    expect(run(['--check']).status).toBe(0)
  })

  it('does not extend a reviewed npm licence to another version', () => {
    reviewedNpm()
    for (const dir of ['.', 'frontend']) {
      const lock = join(root, dir, 'pnpm-lock.yaml')
      writeFileSync(
        lock,
        readFileSync(lock, 'utf8').replace('off-platform@1.0.0', 'off-platform@1.0.1'),
      )
    }
    expect(run().status).toBe(0)
    expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toContain(
      '| off-platform | 1.0.1 | npm | not declared | requires review |',
    )
  })

  it.each(['missing', 'modified'])(
    'rejects a %s reviewed npm text without writing notices',
    (state) => {
      const source = reviewedNpm()
      if (state === 'missing') rmSync(join(source, 'reviewed-LICENSE'))
      else writeFileSync(join(source, 'reviewed-LICENSE'), 'changed')
      expect(run().status).not.toBe(0)
      expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toBe(SENTINEL)
    },
  )

  it.each(['declaration', 'text'])('rejects a conflicting installed npm %s', (kind) => {
    reviewedNpm()
    const dir = join(
      root,
      'frontend/node_modules/.pnpm/off-platform@1.0.0/node_modules/off-platform',
    )
    mkdirSync(dir, { recursive: true })
    writeFileSync(
      join(dir, 'package.json'),
      JSON.stringify({
        name: 'off-platform',
        version: '1.0.0',
        license: kind === 'declaration' ? 'Apache-2.0' : 'MIT',
      }),
    )
    if (kind === 'text') writeFileSync(join(dir, 'LICENSE'), 'Unexpected text')
    expect(run().status).not.toBe(0)
    expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toBe(SENTINEL)
  })

  it.each([
    ['.', 'missing'],
    ['frontend', 'missing'],
    ['.', 'empty'],
    ['frontend', 'empty'],
    ['.', 'unrelated'],
    ['frontend', 'unrelated'],
  ])('refuses a %s installation that is %s without replacing notices', (dir, state) => {
    const store = join(root, dir, 'node_modules/.pnpm')
    rmSync(store, { recursive: true })
    if (state !== 'missing') mkdirSync(store)
    if (state === 'unrelated') {
      const stale = join(store, 'example@0.9.0/node_modules/example')
      mkdirSync(stale, { recursive: true })
      writeFileSync(join(stale, 'package.json'), '{"name":"example","version":"0.9.0"}')
    }

    const result = run()
    expect(result.error).toBeUndefined()
    expect(result.status, result.stderr).toBe(1)
    expect(result.stderr).toContain(`no installed locked npm packages in ${store}`)
    expect(result.stderr).toContain(
      dir === '.'
        ? 'pnpm install --frozen-lockfile'
        : 'pnpm --dir frontend install --frozen-lockfile',
    )
    expect(existsSync(join(root, 'cargo-called'))).toBe(false)
    expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toBe(SENTINEL)
  })

  it('also refuses --check before using the registry when an install is missing', () => {
    rmSync(join(root, 'frontend/node_modules'), { recursive: true })
    rmSync(join(root, 'cargo'), { recursive: true })
    const result = run(['--check'])
    expect(result.error).toBeUndefined()
    expect(result.status, result.stderr).toBe(1)
    expect(result.stderr).toContain('no installed locked npm packages')
    expect(existsSync(join(root, 'cargo-called'))).toBe(false)
    expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toBe(SENTINEL)
  })

  it('keeps established licence text order when usage counts differ', () => {
    const digest = (s: string) => createHash('sha256').update(s).digest('hex')
    const crate = digest('Example crate licence text.\n')
    const npm = digest('Example npm licence text.\n')
    writeFileSync(
      join(root, 'scripts/third-party-license-order.json'),
      JSON.stringify([crate, npm]),
    )
    const result = run()
    expect(result.status, result.stderr).toBe(0)
    const notices = readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')
    expect(notices.indexOf(`SHA-256 \`${crate}\``)).toBeLessThan(
      notices.indexOf(`SHA-256 \`${npm}\``),
    )
  })

  it.each(
    ['0.38.0', '0.41.0'].flatMap((version) =>
      [false, true].map((changed) => [version, changed] as const),
    ),
  )('checks selectors %s reference; changed=%s', (version, changed) => {
    const mpl = 'fab3dd6bdab226f1c08630b1dd917e11fcb4ec5e1e020e2c16f83a0a13863e85'
    const realNotices = readFileSync(
      fileURLToPath(new URL('../../THIRD-PARTY-NOTICES.md', import.meta.url)),
      'utf8',
    )
    const afterHash = realNotices.split(`SHA-256 \`${mpl}\``)[1] ?? ''
    const section = afterHash.slice(afterHash.indexOf('\n\n') + 2)
    const text = /^(`{3,})text\n([\s\S]*?)\n\1\n?/.exec(section)?.[2]
    expect(text).toBeDefined()
    const licence = `${text ?? ''}\n`
    expect(createHash('sha256').update(licence).digest('hex')).toBe(mpl)
    for (const [name, packageVersion] of [
      ['selectors', version],
      ['cssparser', '0.38.0'],
    ]) {
      const dir = join(root, `cargo/registry/src/test/${name}-${packageVersion}`)
      mkdirSync(dir, { recursive: true })
      writeFileSync(
        join(dir, 'Cargo.toml'),
        `[package]\nname = "${name}"\nversion = "${packageVersion}"\nlicense = "MPL-2.0"\n`,
      )
    }
    writeFileSync(
      join(root, 'cargo/registry/src/test/cssparser-0.38.0/LICENSE'),
      changed ? `${licence}changed` : licence,
    )
    const packages = [
      ['example-crate', '1.0.0'],
      ['selectors', version],
      ['cssparser', '0.38.0'],
    ]
    writeFileSync(
      join(root, 'Cargo.lock'),
      packages
        .map(
          ([name, version]) =>
            `[[package]]\nname = "${name}"\nversion = "${version}"\nsource = "registry+test"\n`,
        )
        .join(''),
    )
    const metadata = {
      packages: packages.map(([name, version]) => ({ name, version, source: 'registry+test' })),
    }
    writeFileSync(
      join(root, 'bin/cargo'),
      `#!/bin/sh\nprintf '%s\n' '${JSON.stringify(metadata)}'\n`,
      { mode: 0o755 },
    )
    const result = run()
    if (changed) {
      expect(result.status).toBe(1)
      expect(result.stderr).toContain('verified MPL-2.0 reference is missing or changed')
      expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toBe(SENTINEL)
    } else {
      expect(result.status, result.stderr).toBe(0)
      const notices = readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')
      expect(notices).toMatch(
        new RegExp(
          `\\| selectors \\| ${version.replaceAll('.', '\\.')} \\| crate \\| MPL-2\\.0 \\| T\\d+ \\|`,
        ),
      )
      expect(notices).toContain(`selectors ${version} declares MPL-2.0 but ships no licence file`)
      expect(notices).not.toContain(`- selectors ${version} (crate, registry)`)
      expect(run(['--check']).status).toBe(0)
    }
  })

  it.each(['ok', 'missing notice', 'changed notice', 'changed licence'])(
    'checks Typst source files: %s',
    (state) => {
      const digest = '62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a'
      const realNotices = readFileSync(
        fileURLToPath(new URL('../../THIRD-PARTY-NOTICES.md', import.meta.url)),
        'utf8',
      )
      const afterHash = realNotices.split(`SHA-256 \`${digest}\``)[1] ?? ''
      const section = afterHash.slice(afterHash.indexOf('\n\n') + 2)
      const text = /^(`{3,})text\n([\s\S]*?)\n\1\n?/.exec(section)?.[2]
      expect(text).toBeDefined()
      const licence = `${text ?? ''}\n`
      expect(createHash('sha256').update(licence).digest('hex')).toBe(digest)
      const names = [
        'typst',
        'typst-eval',
        'typst-html',
        'typst-layout',
        'typst-library',
        'typst-macros',
        'typst-pdf',
        'typst-realize',
        'typst-svg',
        'typst-syntax',
        'typst-timing',
        'typst-utils',
        'typst-assets',
      ]
      const packages = [['example-crate', '1.0.0'], ...names.map((name) => [name, '0.15.1'])]
      for (const name of names) {
        const dir = join(root, `cargo/registry/src/test/${name}-0.15.1`)
        mkdirSync(dir, { recursive: true })
        writeFileSync(
          join(dir, 'Cargo.toml'),
          `[package]\nname = "${name}"\nversion = "0.15.1"\nlicense = "Apache-2.0"\n`,
        )
        if (name === 'typst-assets')
          writeFileSync(
            join(dir, 'LICENSE'),
            state === 'changed licence' ? `${licence}changed` : licence,
          )
      }
      const sources = join(root, 'scripts/third-party-sources')
      mkdirSync(sources, { recursive: true })
      const noticePath = join(sources, 'typst-0.15.1-NOTICE')
      if (state !== 'missing notice') {
        copyFileSync(
          fileURLToPath(
            new URL('../../scripts/third-party-sources/typst-0.15.1-NOTICE', import.meta.url),
          ),
          noticePath,
        )
        if (state === 'changed notice') writeFileSync(noticePath, 'changed')
      }
      writeFileSync(
        join(root, 'Cargo.lock'),
        packages
          .map(
            ([name, version]) =>
              `[[package]]\nname = "${name}"\nversion = "${version}"\nsource = "registry+test"\n`,
          )
          .join(''),
      )
      const metadata = {
        packages: packages.map(([name, version]) => ({ name, version, source: 'registry+test' })),
      }
      writeFileSync(
        join(root, 'bin/cargo'),
        `#!/bin/sh\nprintf '%s\n' '${JSON.stringify(metadata)}'\n`,
        { mode: 0o755 },
      )
      const result = run()
      if (state !== 'ok') {
        expect(result.status).toBe(1)
        expect(result.stderr).toContain(
          'verified Typst licence or upstream NOTICE is missing or changed',
        )
        expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toBe(SENTINEL)
      } else {
        expect(result.status, result.stderr).toBe(0)
        const notices = readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')
        for (const name of names.filter((name) => name !== 'typst-assets')) {
          expect(notices).toMatch(
            new RegExp(
              `\\| ${name} \\| 0\\.15\\.1 \\| crate \\| Apache-2\\.0 \\| T\\d+, T\\d+ \\|`,
            ),
          )
          expect(notices).not.toContain(`- ${name} 0.15.1 (crate, registry)`)
        }
        expect(notices).toContain(
          '1778244777547c281b6f5fa9fc0c18ab21f8d4491c803f64e09046800f5fcb26',
        )
        expect(run(['--check']).status).toBe(0)
      }
    },
  )

  it.each([false, true])(
    'keeps objc2 in review and verifies its upstream policy; changed=%s',
    (changed) => {
      const dir = join(root, 'cargo/registry/src/test/objc2-foundation-0.3.2')
      mkdirSync(dir, { recursive: true })
      writeFileSync(
        join(dir, 'Cargo.toml'),
        '[package]\nname = "objc2-foundation"\nversion = "0.3.2"\nlicense = "MIT"\n',
      )
      const sources = join(root, 'scripts/third-party-sources')
      mkdirSync(sources, { recursive: true })
      const policy = join(sources, 'objc2-LICENSE.md')
      copyFileSync(
        fileURLToPath(
          new URL('../../scripts/third-party-sources/objc2-LICENSE.md', import.meta.url),
        ),
        policy,
      )
      if (changed) writeFileSync(policy, 'changed')
      writeFileSync(
        join(root, 'Cargo.lock'),
        '[[package]]\nname = "example-crate"\nversion = "1.0.0"\nsource = "registry+test"\n[[package]]\nname = "objc2-foundation"\nversion = "0.3.2"\nsource = "registry+test"\n',
      )
      const metadata = {
        packages: [
          { name: 'example-crate', version: '1.0.0', source: 'registry+test' },
          { name: 'objc2-foundation', version: '0.3.2', source: 'registry+test' },
        ],
      }
      writeFileSync(
        join(root, 'bin/cargo'),
        `#!/bin/sh\nprintf '%s\n' '${JSON.stringify(metadata)}'\n`,
        { mode: 0o755 },
      )
      const result = run()
      if (changed) {
        expect(result.status).toBe(1)
        expect(result.stderr).toContain('verified objc2 upstream policy is missing or changed')
        expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toBe(SENTINEL)
      } else {
        expect(result.status, result.stderr).toBe(0)
        const notices = readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')
        expect(notices).toContain('| objc2-foundation | 0.3.2 | crate | MIT | requires review |')
        expect(notices).toContain(
          '7f976f7e9cb2d87df7230606feb932c3f21ac0e664045a775b600046ff850c54',
        )
        expect(notices).toContain('notes uncertainty concerning Apple SDK-derived code')
        expect(run(['--check']).status).toBe(0)
      }
    },
  )

  it.each(['ok', 'missing specta', 'changed specta', 'missing tauri', 'changed tauri'])(
    'checks Specta licences: %s',
    (state) => {
      const groups = [
        {
          file: 'specta-e7fd23f-LICENSE',
          key: 'specta',
          packages: [
            ['specta', '2.0.0-rc.25'],
            ['specta-macros', '2.0.0-rc.25'],
            ['specta-serde', '0.0.12'],
            ['specta-typescript', '0.0.12'],
            ['specta-util', '0.0.12'],
          ],
        },
        {
          file: 'tauri-specta-e5e578f-LICENSE',
          key: 'tauri',
          packages: [
            ['tauri-specta', '2.0.0-rc.25'],
            ['tauri-specta-macros', '2.0.0-rc.25'],
          ],
        },
      ]
      const sources = join(root, 'scripts/third-party-sources')
      mkdirSync(sources, { recursive: true })
      for (const group of groups) {
        const path = join(sources, group.file)
        if (state !== `missing ${group.key}`) {
          copyFileSync(
            fileURLToPath(
              new URL(`../../scripts/third-party-sources/${group.file}`, import.meta.url),
            ),
            path,
          )
          if (state === `changed ${group.key}`) writeFileSync(path, 'changed')
        }
        for (const [name, version] of group.packages) {
          const dir = join(root, `cargo/registry/src/test/${name}-${version}`)
          mkdirSync(dir, { recursive: true })
          writeFileSync(
            join(dir, 'Cargo.toml'),
            `[package]\nname = "${name}"\nversion = "${version}"\nlicense = "MIT"\n`,
          )
        }
      }
      const packages = [['example-crate', '1.0.0'], ...groups.flatMap((group) => group.packages)]
      writeFileSync(
        join(root, 'Cargo.lock'),
        packages
          .map(
            ([name, version]) =>
              `[[package]]\nname = "${name}"\nversion = "${version}"\nsource = "registry+test"\n`,
          )
          .join(''),
      )
      const metadata = {
        packages: packages.map(([name, version]) => ({ name, version, source: 'registry+test' })),
      }
      writeFileSync(
        join(root, 'bin/cargo'),
        `#!/bin/sh\nprintf '%s\n' '${JSON.stringify(metadata)}'\n`,
        { mode: 0o755 },
      )
      const result = run()
      if (state !== 'ok') {
        expect(result.status).toBe(1)
        expect(result.stderr).toContain('is missing or changed')
        expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toBe(SENTINEL)
      } else {
        expect(result.status, result.stderr).toBe(0)
        const notices = readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')
        for (const [name, version] of groups.flatMap((group) => group.packages)) {
          expect(notices).toMatch(
            new RegExp(
              `\\| ${name} \\| ${version.replaceAll('.', '\\.')} \\| crate \\| MIT \\| T\\d+ \\|`,
            ),
          )
          expect(notices).not.toContain(`- ${name} ${version} (crate, registry)`)
        }
        expect(notices).toContain('The source file does not end with a newline')
        expect(notices).toContain('The source file ends with a newline')
        expect(run(['--check']).status).toBe(0)
      }
    },
  )

  it('keeps available texts and legitimate off-platform review rows, then checks the result', () => {
    const result = run()
    expect(result.error).toBeUndefined()
    expect(result.status, result.stderr).toBe(0)
    expect(result.stdout).toContain('5 packages, 2 texts, 2 requires review')
    const notices = readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')
    expect(notices).toContain('Example crate licence text.')
    expect(notices).toContain('Example npm licence text.')
    expect(notices).toContain('| off-platform | 1.0.0 | npm | not declared | requires review |')
    expect(notices).toContain(
      '| off-platform | 1.0.0 | npm (root) | not declared | requires review |',
    )
    const checked = run(['--check'])
    expect(checked.error).toBeUndefined()
    expect(checked.status, checked.stderr).toBe(0)
    expect(checked.stdout).toContain('THIRD-PARTY-NOTICES.md is current')
    expect(readFileSync(join(root, 'THIRD-PARTY-NOTICES.md'), 'utf8')).toBe(notices)
  })
})
