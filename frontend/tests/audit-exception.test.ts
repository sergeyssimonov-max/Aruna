import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

/**
 * The one audit exception does not outlive its reason.
 *
 * `pnpm-workspace.yaml` excepts GHSA-3wwx-pv8p-q78v (undici, owner's decision
 * of 2026-09-29): the vulnerable code comes only with test tools, and the
 * fixed releases are not old enough to take (specification 5.1). An exception
 * like that is right for a week and wrong for ever after, and nothing would
 * say so – `pnpm audit` goes quiet either way. So this fails on the day the
 * exception is due to go, and as soon as every undici the lockfile resolves is
 * already fixed, whichever comes first.
 */

const FRONTEND = join(dirname(fileURLToPath(import.meta.url)), '..')

const GHSA = 'GHSA-3wwx-pv8p-q78v'

/** The day the exception is due to be gone: the maintenance task is on 2026-10-05. */
const DUE = '2026-10-06'

/** The first fixed release of each undici line the advisory names. */
const FIXED: Record<number, [number, number, number]> = {
  6: [6, 28, 1],
  7: [7, 29, 1],
  8: [8, 10, 2],
}

/** Every undici version the lockfile resolves. */
function undiciVersions(lock: string): [number, number, number][] {
  const found = new Map<string, [number, number, number]>()
  for (const m of lock.matchAll(/^ {2}'?undici@(\d+)\.(\d+)\.(\d+)'?:/gm)) {
    found.set(`${m[1]}.${m[2]}.${m[3]}`, [Number(m[1]), Number(m[2]), Number(m[3])])
  }
  return [...found.values()]
}

function atOrPast(version: [number, number, number], fix: [number, number, number]): boolean {
  for (let i = 0; i < 3; i++) {
    if (version[i] !== fix[i]) return version[i] > fix[i]
  }
  return true
}

/**
 * Why the exception has to go, or `null` while it still stands. A line the
 * advisory does not name is taken to be fixed.
 */
function expired(today: string, lock: string): string | null {
  if (today >= DUE) {
    return `${GHSA} was to be removed by ${DUE}; today is ${today}`
  }
  const versions = undiciVersions(lock)
  if (versions.length === 0) {
    return `the lockfile resolves no undici: ${GHSA} excepts nothing`
  }
  const vulnerable = versions.filter((v) => FIXED[v[0]] && !atOrPast(v, FIXED[v[0]]))
  if (vulnerable.length === 0) {
    return `every undici in the lockfile is fixed (${versions.map((v) => v.join('.')).join(', ')}): remove ${GHSA}`
  }
  return null
}

/** Today in local time, as the owner's calendar has it. */
function today(): string {
  const now = new Date()
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

describe(`the audit exception ${GHSA}`, () => {
  it('is the only one, and still has its reason', () => {
    const workspace = readFileSync(join(FRONTEND, 'pnpm-workspace.yaml'), 'utf8')
    const ignored = [...workspace.matchAll(/GHSA-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}/g)].map(
      (m) => m[0],
    )
    expect(new Set(ignored)).toEqual(new Set([GHSA]))
    expect(workspace).not.toMatch(/auditConfig|ignoreGhsas|ignoreCves/)

    const lock = readFileSync(join(FRONTEND, 'pnpm-lock.yaml'), 'utf8')
    expect(expired(today(), lock)).toBeNull()
  })

  it('would fail on the due day, and once every undici is fixed', () => {
    const vulnerable = '  undici@6.28.0:\n  undici@7.29.0:\n  undici@8.10.0:\n'
    const fixed = '  undici@6.28.1:\n  undici@7.29.1:\n  undici@8.10.2:\n'
    const partly = '  undici@6.28.1:\n  undici@7.29.0:\n  undici@8.10.2:\n'

    expect(expired('2026-10-05', vulnerable)).toBeNull()
    expect(expired('2026-10-06', vulnerable)).toMatch(/was to be removed/)
    expect(expired('2026-10-05', fixed)).toMatch(/every undici in the lockfile is fixed/)
    expect(expired('2026-10-05', partly)).toBeNull()
    expect(expired('2026-10-05', '')).toMatch(/resolves no undici/)
  })
})
