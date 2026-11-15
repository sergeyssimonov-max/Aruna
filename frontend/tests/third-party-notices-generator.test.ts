import { spawnSync } from 'node:child_process'
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
