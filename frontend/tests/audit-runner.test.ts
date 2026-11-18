import { spawnSync } from 'node:child_process'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

const SCRIPT = fileURLToPath(new URL('../scripts/audit.sh', import.meta.url))
const GUARD =
  'exec vitest run tests/picomatch-security.test.ts --project node --passWithNoTests=false'
const WORKSPACE = 'audit:\n  ignore:\n    - GHSA-aaaa-bbbb-cccc\n'

// Exercise the actual shell entry point at its process boundary. The fake pnpm
// records calls; it does not run or pretend to replace the real security tests.
describe('the conditional audit entry point', { timeout: 15_000 }, () => {
  let root: string
  let bin: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'aruna audit '))
    bin = join(root, 'bin')
    mkdirSync(bin)
    mkdirSync(join(root, 'scripts'))
    mkdirSync(join(root, 'patches'))
    mkdirSync(join(root, 'tmp'))
    writeFileSync(join(root, 'package.json'), '{"name":"audit-fixture","private":true}\n')
    writeFileSync(join(root, 'pnpm-lock.yaml'), 'lockfileVersion: 9.0\n')
    writeFileSync(join(root, 'pnpm-workspace.yaml'), WORKSPACE)
    copyFileSync(SCRIPT, join(root, 'scripts/audit.sh'))
    writeFileSync(
      join(bin, 'pnpm'),
      `#!/bin/bash
set -eu
printf '%s\\n' "$*" >> "$ARUNA_TEST_CALLS"
pwd > "$ARUNA_TEST_CWD"
case "$1" in
  exec)
    if [[ \${ARUNA_TEST_GUARD_SIGNAL:-0} == 1 ]]; then kill -TERM $$; fi
    exit "\${ARUNA_TEST_GUARD_CODE:-0}"
    ;;
  --dir)
    [[ "$3" == audit && $# == 3 ]] || exit 99
    printf '%s\\n' "$2" > "$ARUNA_TEST_AUDITDIR"
    cp "$2/pnpm-workspace.yaml" "$ARUNA_TEST_VIEW"
    cp "$2/pnpm-lock.yaml" "$ARUNA_TEST_LOCK"
    if [[ \${ARUNA_TEST_AUDIT_SLEEP:-0} == 1 ]]; then exec /bin/sleep 2; fi
    exit "\${ARUNA_TEST_AUDIT_CODE:-0}"
    ;;
  *) exit 99 ;;
esac
`,
      { mode: 0o755 },
    )
  })

  afterEach(() => rmSync(root, { recursive: true, force: true }))

  function run(args: string[] = [], env: NodeJS.ProcessEnv = {}) {
    return spawnSync('/bin/bash', [join(root, 'scripts/audit.sh'), ...args], {
      cwd: tmpdir(),
      encoding: 'utf8',
      timeout: 10_000,
      env: {
        ...process.env,
        PATH: `${bin}:${dirname(process.execPath)}:/usr/bin:/bin`,
        TMPDIR: join(root, 'tmp'),
        ARUNA_TEST_CALLS: join(root, 'calls'),
        ARUNA_TEST_CWD: join(root, 'cwd'),
        ARUNA_TEST_AUDITDIR: join(root, 'auditdir'),
        ARUNA_TEST_VIEW: join(root, 'audit-workspace.yaml'),
        ARUNA_TEST_LOCK: join(root, 'audit-lock.yaml'),
        ARUNA_AUDIT_TIMEOUT: '30',
        ...env,
      },
    })
  }

  function calls() {
    return existsSync(join(root, 'calls'))
      ? readFileSync(join(root, 'calls'), 'utf8').trim().split('\n')
      : []
  }

  function verifiedAudit() {
    const dir = readFileSync(join(root, 'auditdir'), 'utf8').trim()
    expect(calls()).toEqual([GUARD, `--dir ${dir} audit`])
    expect(readFileSync(join(root, 'audit-workspace.yaml'), 'utf8')).toBe(
      WORKSPACE.replace('  ignore:\n', '  ignore:\n    - GHSA-fc5q-39xc-vjpf\n'),
    )
    expect(readFileSync(join(root, 'pnpm-workspace.yaml'), 'utf8')).toBe(WORKSPACE)
    expect(readFileSync(join(root, 'audit-lock.yaml'))).toEqual(
      readFileSync(join(root, 'pnpm-lock.yaml')),
    )
    expect(existsSync(dir)).toBe(false)
  }

  it('keeps both package scripts connected to the guarded entry point', () => {
    const manifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
    expect(manifest.scripts.audit).toBe('./scripts/audit.sh')
    expect(manifest.scripts['audit:strict']).toBe('./scripts/audit.sh --strict')
  })

  it.each([{ mode: [] }, { mode: ['--strict'] }])(
    'verifies before granting only the named exception: $mode',
    ({ mode }) => {
      const result = run(mode)
      expect(result.error).toBeUndefined()
      expect(result.status, result.stderr).toBe(0)
      verifiedAudit()
      expect(readFileSync(join(root, 'cwd'), 'utf8').trim()).toBe(root)
    },
  )

  for (const mode of [[], ['--strict']]) {
    it.each([1, 14, 142])(
      `does not audit when the guard fails with %i (${mode.join()})`,
      (code) => {
        const result = run(mode, { ARUNA_TEST_GUARD_CODE: String(code) })
        expect(result.error).toBeUndefined()
        expect(result.status).toBe(code)
        expect(calls()).toEqual([GUARD])
        expect(result.stderr).toContain('исправление picomatch не подтверждено')
        expect(result.stdout).not.toContain('АУДИТ НЕ ВЫПОЛНЕН')
      },
    )
  }

  it('does not grant an exception when the guard is terminated', () => {
    const result = run(['--strict'], { ARUNA_TEST_GUARD_SIGNAL: '1' })
    expect(result.status).not.toBe(0)
    expect(calls()).toEqual([GUARD])
  })

  it.each([1, 14, 142])(
    'strict mode propagates audit failure %i without a timeout bypass',
    (code) => {
      const result = run(['--strict'], {
        ARUNA_TEST_AUDIT_CODE: String(code),
        ARUNA_AUDIT_TIMEOUT: '1',
      })
      expect(result.status).toBe(code)
      verifiedAudit()
      expect(result.stdout).not.toContain('АУДИТ НЕ ВЫПОЛНЕН')
    },
  )

  it('does not swallow a new finding in bounded mode', () => {
    const result = run([], { ARUNA_TEST_AUDIT_CODE: '1' })
    expect(result.status).toBe(1)
    verifiedAudit()
  })

  it('retains the existing explicit registry timeout notice only after a successful guard', () => {
    const result = run([], { ARUNA_TEST_AUDIT_SLEEP: '1', ARUNA_AUDIT_TIMEOUT: '1' })
    expect(result.error).toBeUndefined()
    expect(result.status).toBe(0)
    verifiedAudit()
    expect(result.stdout).toContain('АУДИТ НЕ ВЫПОЛНЕН')
    expect(result.stdout).toContain('audit:strict')
  })

  it('rejects extra audit arguments instead of allowing additional ignores', () => {
    const result = run(['--strict', '--ignore-unfixable'])
    expect(result.status).toBe(2)
    expect(calls()).toEqual([])
  })

  it('rejects invalid timeout input without interpolating it into Perl', () => {
    const result = run([], { ARUNA_AUDIT_TIMEOUT: '1; exit 0' })
    expect(result.status).toBe(2)
    expect(calls()).toEqual([GUARD])
  })

  it('refuses an unexpected workspace policy and removes the temporary directory', () => {
    writeFileSync(join(root, 'pnpm-workspace.yaml'), 'audit: {}\n')
    const result = run(['--strict'])
    expect(result.status).not.toBe(0)
    expect(calls()).toEqual([GUARD])
    expect(readdirSync(join(root, 'tmp'))).toEqual([])
  })
})
