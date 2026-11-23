import { spawnSync } from 'node:child_process'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

const SCRIPT = fileURLToPath(new URL('../../scripts/build-release.sh', import.meta.url))
const NODE_VERSION = readFileSync(new URL('../../.node-version', import.meta.url), 'utf8').trim()
const EPOCH = '1704067200'
const VERSION = '7.8.9'
const DMG_NAME = `Aruna_${VERSION}_universal.dmg`

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
  let app: string
  let dmgDir: string

  function mock(name: string, body: string) {
    writeFileSync(join(bin, name), `#!/bin/bash\nset -eu\n${body}\n`, { mode: 0o755 })
  }

  beforeEach(() => {
    root = realpathSync(mkdtempSync(join(tmpdir(), 'aruna release ')))
    bin = join(root, 'bin')
    const bundle = join(root, 'target/universal-apple-darwin/release/bundle')
    app = join(bundle, 'macos/Aruna.app')
    dmgDir = join(bundle, 'dmg')
    mkdirSync(bin)
    mkdirSync(join(root, 'scripts'))
    copyFileSync(SCRIPT, join(root, 'scripts/build-release.sh'))
    writeFileSync(join(root, '.node-version'), NODE_VERSION)
    writeFileSync(
      join(root, 'Info.plist'),
      `<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleShortVersionString</key><string>${VERSION}</string></dict></plist>`,
    )
    writeFileSync(
      join(root, 'scripts/build-repro-dmg.py'),
      `import json, os, sys
from pathlib import Path
app, output = map(Path, sys.argv[1:])
assert app.is_dir() and not output.exists()
Path("packer.json").write_text(json.dumps({
    "app": str(app.resolve()), "output": str(output.resolve()),
    "epoch": os.environ["SOURCE_DATE_EPOCH"],
    "isolated": bool(sys.flags.isolated), "no_bytecode": bool(sys.dont_write_bytecode),
    "private_stage": output.parent.stat().st_mode & 0o077 == 0,
}))
if os.environ.get("ARUNA_TEST_SKIP_OUTPUT") != "1":
    output.write_bytes(b"new image")
sys.exit(int(os.environ.get("ARUNA_TEST_PACKER_EXIT", "0")))
`,
    )
    mock('xcodebuild', 'printf "%s\\n" "${ARUNA_TEST_XCODE}" "Build version 15C500b"')
    mock('xcrun', 'printf "%s\\n" "${ARUNA_TEST_SDK}"')
    mock('node', 'printf "v%s\\n" "${ARUNA_TEST_NODE}"')
    mock(
      'rustc',
      'case "$1" in --print) printf "%s\\n" "$ARUNA_TEST_SYSROOT" ;; -vV) printf "commit-hash: test-commit\\n" ;; *) exit 2 ;; esac',
    )
    mock(
      'tauri',
      `printf '%s\\0' "$@" > "$ARUNA_TEST_ARGS"
printf '%s' "\${VITE_E2E-unset}" > "$ARUNA_TEST_E2E"
printf '%s' "$CARGO_ENCODED_RUSTFLAGS" > "$ARUNA_TEST_FLAGS"
if [ "\${ARUNA_TEST_TAURI_EXIT:-0}" != 0 ]; then exit "$ARUNA_TEST_TAURI_EXIT"; fi
if [ "\${ARUNA_TEST_SKIP_APP:-0}" != 1 ]; then
  output="\${CARGO_TARGET_DIR:-$PWD/target}/universal-apple-darwin/release/bundle/macos/Aruna.app"
  mkdir -p "$output/Contents"
  cp Info.plist "$output/Contents/Info.plist"
fi`,
    )
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  function run(args: string[] = [], extra: NodeJS.ProcessEnv = {}) {
    const env: NodeJS.ProcessEnv = { ...process.env }
    for (const name of [
      'VITE_E2E',
      'TAURI_CONFIG',
      'RUSTFLAGS',
      'CARGO_ENCODED_RUSTFLAGS',
      'CARGO_TARGET_DIR',
      'GIT_DIR',
      'GIT_WORK_TREE',
    ]) {
      delete env[name]
    }
    const result = spawnSync('/bin/bash', [join(root, 'scripts/build-release.sh'), ...args], {
      encoding: 'utf8',
      timeout: 15_000,
      env: {
        ...env,
        PATH: `${bin}:/usr/bin:/bin`,
        CARGO_HOME: join(root, 'cargo home'),
        SOURCE_DATE_EPOCH: EPOCH,
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
    expect(result.error).toBeUndefined()
    expect(result.signal).toBeNull()
    return result
  }

  function builtArgs() {
    return readFileSync(join(root, 'args'), 'utf8').split('\0').slice(0, -1)
  }

  function packed() {
    return existsSync(join(root, 'packer.json'))
  }

  function checkPackedImage(epoch: string) {
    const call = JSON.parse(readFileSync(join(root, 'packer.json'), 'utf8'))
    expect(call.app).toBe(app)
    expect(call.epoch).toBe(epoch)
    expect(call).toMatchObject({ isolated: true, no_bytecode: true, private_stage: true })
    expect(dirname(dirname(call.output))).toBe(dmgDir)
    expect(basename(call.output)).toBe(DMG_NAME)
    expect(existsSync(dirname(call.output))).toBe(false)
    expect(readFileSync(join(dmgDir, DMG_NAME), 'utf8')).toBe('new image')
  }

  function makeCheckout() {
    const git = (args: string[]) => {
      const result = spawnSync('git', args, {
        cwd: root,
        encoding: 'utf8',
        env: {
          ...process.env,
          GIT_CONFIG_GLOBAL: '/dev/null',
          GIT_CONFIG_NOSYSTEM: '1',
          GIT_AUTHOR_DATE: `@${EPOCH} +0000`,
          GIT_COMMITTER_DATE: `@${EPOCH} +0000`,
        },
      })
      expect(result.status, result.stderr).toBe(0)
    }
    git(['init', '--quiet'])
    git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      '-c',
      'core.hooksPath=/dev/null',
      'commit',
      '--quiet',
      '--allow-empty',
      '-m',
      'fixture',
    ])
  }

  function refused(args: string[], env: NodeJS.ProcessEnv = {}) {
    const result = run(args, env)
    expect(result.status, result.stderr).toBe(1)
    expect(result.stderr).toContain('build-release:')
    expect(existsSync(join(root, 'args'))).toBe(false)
    expect(packed()).toBe(false)
  }

  it('builds the universal release with a frozen Cargo graph and no E2E bridge', () => {
    const result = run()
    expect(result.status, result.stderr).toBe(0)
    expect(builtArgs()).toEqual([
      'build',
      '--target',
      'universal-apple-darwin',
      '--bundles',
      'app',
      '--',
      '--locked',
    ])
    checkPackedImage(EPOCH)
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
    { args: ['--bundles', 'app', '--ci'], forwarded: ['--ci'], image: false },
    { args: ['--bundles', 'app', 'dmg', '--ci'], forwarded: ['--ci'], image: true },
    { args: ['--bundles=app,dmg', '--ci'], forwarded: ['--ci'], image: true },
    { args: ['--ci', '--bundles=dmg,app', '-v'], forwarded: ['--ci', '-v'], image: true },
    { args: ['--bundles=dmg'], forwarded: [], image: true },
    { args: ['--bundles=app', '--bundles=dmg'], forwarded: [], image: true },
    { args: ['-b', 'dmg'], forwarded: [], image: true },
    { args: ['-b', 'app', '-vv'], forwarded: ['-vv'], image: false },
  ])('preserves Tauri options and the image selection for $args', ({ args, forwarded, image }) => {
    const result = run(args, { SOURCE_DATE_EPOCH: image ? EPOCH : undefined })
    expect(result.status, result.stderr).toBe(0)
    expect(builtArgs()).toEqual([
      'build',
      '--target',
      'universal-apple-darwin',
      ...forwarded,
      '--bundles',
      'app',
      '--',
      '--locked',
    ])
    expect(packed()).toBe(image)
    if (image) checkPackedImage(EPOCH)
  })

  it('accepts an explicit --locked without passing it as a Tauri option', () => {
    const result = run(['--locked', '--ci'])
    expect(result.status, result.stderr).toBe(0)
    expect(builtArgs()).toEqual([
      'build',
      '--target',
      'universal-apple-darwin',
      '--ci',
      '--bundles',
      'app',
      '--',
      '--locked',
    ])
  })

  it.each(['--no-bundle', '--help', '-h', '--version', '-V'])(
    'passes %s to Tauri without making an image',
    (flag) => {
      const result = run([flag], { SOURCE_DATE_EPOCH: undefined })
      expect(result.status, result.stderr).toBe(0)
      expect(builtArgs()).toEqual([
        'build',
        '--target',
        'universal-apple-darwin',
        flag,
        '--',
        '--locked',
      ])
      expect(packed()).toBe(false)
    },
  )

  it('uses the timestamp of the checked-out commit when no epoch is supplied', () => {
    makeCheckout()
    const result = run([], { SOURCE_DATE_EPOCH: undefined })
    expect(result.status, result.stderr).toBe(0)
    checkPackedImage(EPOCH)
  })

  it('refuses an explicit epoch that differs from the checked-out commit', () => {
    makeCheckout()
    const result = run([], { SOURCE_DATE_EPOCH: '1704067201' })
    expect(result.status).not.toBe(0)
    expect(packed()).toBe(false)
  })

  it.each([undefined, 'not-an-epoch', '-1'])(
    'refuses an archive without a valid epoch (%s)',
    (value) => {
      const result = run([], { SOURCE_DATE_EPOCH: value })
      expect(result.status).not.toBe(0)
      expect(packed()).toBe(false)
    },
  )

  it('stops before packaging when Tauri fails', () => {
    expect(run([], { ARUNA_TEST_TAURI_EXIT: '29' }).status).toBe(29)
    expect(packed()).toBe(false)
  })

  it('requires the named Aruna bundle even if another bundle already exists', () => {
    const unrelated = join(dirname(app), 'Other.app/Contents')
    mkdirSync(unrelated, { recursive: true })
    copyFileSync(join(root, 'Info.plist'), join(unrelated, 'Info.plist'))
    expect(run([], { ARUNA_TEST_SKIP_APP: '1' }).status).not.toBe(0)
    expect(packed()).toBe(false)
  })

  it('refuses a plist without the release version', () => {
    writeFileSync(join(root, 'Info.plist'), '<plist version="1.0"><dict/></plist>')
    expect(run().status).not.toBe(0)
    expect(packed()).toBe(false)
  })

  it('uses a relative Cargo target directory for the application and its image', () => {
    const bundle = join(root, 'target with spaces/universal-apple-darwin/release/bundle')
    app = join(bundle, 'macos/Aruna.app')
    dmgDir = join(bundle, 'dmg')
    const result = run([], { CARGO_TARGET_DIR: 'target with spaces' })
    expect(result.status, result.stderr).toBe(0)
    checkPackedImage(EPOCH)
  })

  it('publishes only the requested image even if another version exists', () => {
    mkdirSync(dmgDir, { recursive: true })
    writeFileSync(join(dmgDir, 'Aruna_1.0.0_universal.dmg'), 'other version')
    writeFileSync(join(dmgDir, DMG_NAME), 'old image')
    const result = run()
    expect(result.status, result.stderr).toBe(0)
    checkPackedImage(EPOCH)
    expect(readFileSync(join(dmgDir, 'Aruna_1.0.0_universal.dmg'), 'utf8')).toBe('other version')
  })

  it.each([
    { extra: { ARUNA_TEST_PACKER_EXIT: '37' }, exit: 37 },
    { extra: { ARUNA_TEST_SKIP_OUTPUT: '1' }, exit: undefined },
  ])(
    'preserves the previous image if the packer fails or leaves no file ($extra)',
    ({ extra, exit }) => {
      mkdirSync(dmgDir, { recursive: true })
      writeFileSync(join(dmgDir, DMG_NAME), 'old image')
      writeFileSync(join(dmgDir, 'unrelated.dmg'), 'leave alone')
      const result = run([], extra)
      if (exit) expect(result.status).toBe(exit)
      else expect(result.status).not.toBe(0)
      expect(packed()).toBe(true)
      expect(readdirSync(dmgDir).sort()).toEqual([DMG_NAME, 'unrelated.dmg'])
      expect(readFileSync(join(dmgDir, DMG_NAME), 'utf8')).toBe('old image')
    },
  )

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
