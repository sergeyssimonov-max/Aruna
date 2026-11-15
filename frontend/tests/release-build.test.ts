import { spawnSync } from 'node:child_process'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

const SCRIPT = fileURLToPath(new URL('../../scripts/build-release.sh', import.meta.url))
const NODE_VERSION = readFileSync(new URL('../../.node-version', import.meta.url), 'utf8').trim()

/**
 * Execute the real release entry point against a toolchain that records the
 * command it would build. No Apple tools, compilation, network or user files
 * are needed. This catches argument forwarding errors a text assertion cannot:
 * --locked must reach Cargo after `--`, and E2E must never reach the builder.
 */
// These are process-boundary checks, not a build-time performance budget.
// Allow process startup while the same host is compiling the Rust workspace.
describe('the release entry point', { timeout: 20_000 }, () => {
  let root: string
  let bin: string

  function mock(name: string, body: string) {
    writeFileSync(join(bin, name), `#!/bin/bash\nset -eu\n${body}\n`, { mode: 0o755 })
  }

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'aruna release '))
    bin = join(root, 'bin')
    mkdirSync(bin)
    mkdirSync(join(root, 'scripts'))
    copyFileSync(SCRIPT, join(root, 'scripts/build-release.sh'))
    writeFileSync(join(root, '.node-version'), NODE_VERSION)
    mock('xcodebuild', 'printf "%s\\n" "${ARUNA_TEST_XCODE}" "Build version 15C500b"')
    mock('xcrun', 'printf "%s\\n" "${ARUNA_TEST_SDK}"')
    mock('node', 'printf "v%s\\n" "${ARUNA_TEST_NODE}"')
    mock(
      'rustc',
      'case "$1" in --print) printf "%s\\n" "$ARUNA_TEST_SYSROOT" ;; -vV) printf "commit-hash: test-commit\\n" ;; *) exit 2 ;; esac',
    )
    mock(
      'tauri',
      'printf "%s\\0" "$@" > "$ARUNA_TEST_ARGS"\nprintf "%s" "${VITE_E2E-unset}" > "$ARUNA_TEST_E2E"\nprintf "%s" "$CARGO_ENCODED_RUSTFLAGS" > "$ARUNA_TEST_FLAGS"',
    )
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  function run(args: string[] = [], extra: NodeJS.ProcessEnv = {}) {
    const env: NodeJS.ProcessEnv = { ...process.env }
    for (const name of ['VITE_E2E', 'TAURI_CONFIG', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']) {
      delete env[name]
    }
    return spawnSync('/bin/bash', [join(root, 'scripts/build-release.sh'), ...args], {
      encoding: 'utf8',
      timeout: 15_000,
      env: {
        ...env,
        PATH: `${bin}:/usr/bin:/bin`,
        CARGO_HOME: join(root, 'cargo home'),
        ARUNA_TEST_XCODE: 'Xcode 15.2',
        ARUNA_TEST_SDK: '14.2',
        ARUNA_TEST_NODE: NODE_VERSION,
        ARUNA_TEST_SYSROOT: join(root, 'rust sysroot'),
        ARUNA_TEST_ARGS: join(root, 'args'),
        ARUNA_TEST_E2E: join(root, 'e2e'),
        ARUNA_TEST_FLAGS: join(root, 'flags'),
        ...extra,
      },
    })
  }

  function builtArgs() {
    return readFileSync(join(root, 'args'), 'utf8').split('\0').slice(0, -1)
  }

  function refused(args: string[], env: NodeJS.ProcessEnv = {}) {
    const result = run(args, env)
    expect(result.error).toBeUndefined()
    expect(result.status, result.stderr).toBe(1)
    expect(result.stderr).toContain('build-release:')
    expect(existsSync(join(root, 'args'))).toBe(false)
  }

  it('builds the universal release with a frozen Cargo graph and no E2E bridge', () => {
    const result = run()
    expect(result.status, result.stderr).toBe(0)
    expect(builtArgs()).toEqual(['build', '--target', 'universal-apple-darwin', '--', '--locked'])
    // Vite gives this process value priority over a developer's .env files.
    expect(readFileSync(join(root, 'e2e'), 'utf8')).toBe('')
    const physicalRoot = readFileSync(join(root, 'flags'), 'utf8').split('\x1f')[0]
    expect(physicalRoot).toMatch(/^--remap-path-prefix=.*aruna release .*=/)
    expect(readFileSync(join(root, 'flags'), 'utf8').split('\x1f').slice(1)).toEqual([
      `--remap-path-prefix=${join(root, 'cargo home')}=/cargo`,
      `--remap-path-prefix=${join(root, 'rust sysroot')}/lib/rustlib/src/rust=/rustc/test-commit`,
      '-Clink-arg=-Wl,-S',
    ])
  })

  it.each([
    ['--bundles', 'app', '--ci'],
    ['--bundles', 'app', 'dmg', '--ci'],
    ['--bundles=app,dmg', '--ci'],
    ['-b', 'app', '-vv'],
  ])('keeps packaging options before Cargo arguments: %j', (...options) => {
    const result = run(options)
    expect(result.status, result.stderr).toBe(0)
    expect(builtArgs()).toEqual([
      'build',
      '--target',
      'universal-apple-darwin',
      ...options,
      '--',
      '--locked',
    ])
  })

  it('accepts an explicit --locked without passing it as a Tauri option', () => {
    const result = run(['--locked', '--ci'])
    expect(result.status, result.stderr).toBe(0)
    expect(builtArgs()).toEqual([
      'build',
      '--target',
      'universal-apple-darwin',
      '--ci',
      '--',
      '--locked',
    ])
  })

  it.each(['1', '0', 'false'])('refuses a truthy Vite E2E string %s before building', (value) => {
    refused([], { VITE_E2E: value })
  })

  it.each([
    ['--features', 'e2e'],
    ['--features=e2e'],
    ['-f', 'e2e'],
    ['-fe2e'],
    ['--', '--features', 'aruna-desktop/e2e'],
    ['--', '--all-features'],
    ['--config', '{"build":{"features":["e2e"]}}'],
    ['-c', 'e2e.json'],
    ['--runner', 'custom-cargo'],
    ['--debug'],
    ['--target', 'aarch64-apple-darwin'],
  ])('refuses release configuration overrides: %j', (...options) => {
    refused(options)
  })

  it('refuses a configuration inherited from another Tauri command', () => {
    refused([], { TAURI_CONFIG: '{"build":{"features":["e2e"]}}' })
  })

  it.each([
    { ARUNA_TEST_NODE: '0.0.0' },
    { ARUNA_TEST_XCODE: 'Xcode 16.0' },
    { ARUNA_TEST_SDK: '15.0' },
    { RUSTFLAGS: '-C opt-level=0' },
    { CARGO_ENCODED_RUSTFLAGS: '-Copt-level=0' },
  ])('keeps the existing toolchain and reproducibility gates: %j', (env) => {
    refused([], env)
  })
})
