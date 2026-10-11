import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { changed, finish, isolate, traces } from '../e2e/isolation.ts'

/**
 * The guard of the E2E contour against the real `~/Library` (owner's decision
 * of 2026-10-11): a run that leaves a file of Aruna's there fails. Held here
 * against a `Library` of its own, so the teeth are shown without touching the
 * real one.
 */
describe('E2E isolation', () => {
  let library: string

  beforeEach(() => {
    library = mkdtempSync(join(tmpdir(), 'aruna-library-'))
    mkdirSync(join(library, 'WebKit'))
    mkdirSync(join(library, 'Application Support', 'Someone Else'), { recursive: true })
    writeFileSync(join(library, 'Application Support', 'Someone Else', 'state'), 'x')
  })

  afterEach(() => {
    rmSync(library, { recursive: true, force: true })
  })

  it('sees only what is Aruna’s', () => {
    mkdirSync(join(library, 'WebKit', 'aruna-desktop', 'WebsiteData'), { recursive: true })
    writeFileSync(join(library, 'Preferences.plist'), 'x')
    mkdirSync(join(library, 'Preferences'))
    writeFileSync(join(library, 'Preferences', 'com.sergeyssimonov.aruna.plist'), 'x')
    writeFileSync(join(library, 'Preferences', 'com.sergeyssimonov.arunafan.plist'), 'x')
    const seen = [...traces(library).keys()].map((p) => p.slice(library.length + 1)).sort()
    expect(seen).toEqual([
      'Preferences/com.sergeyssimonov.aruna.plist',
      'WebKit/aruna-desktop',
      'WebKit/aruna-desktop/WebsiteData',
    ])
  })

  it('names a window state that appeared and a cache that changed', () => {
    mkdirSync(join(library, 'Caches', 'aruna-desktop'), { recursive: true })
    writeFileSync(join(library, 'Caches', 'aruna-desktop', 'blob'), 'a')
    const before = traces(library)
    const state = join(library, 'Application Support', 'com.sergeyssimonov.aruna')
    mkdirSync(state)
    writeFileSync(join(state, '.window-state.json'), '{}')
    writeFileSync(join(library, 'Caches', 'aruna-desktop', 'blob'), 'ab')
    expect(changed(before, traces(library))).toEqual([
      `appeared: ${state}`,
      `appeared: ${join(state, '.window-state.json')}`,
      `changed: ${join(library, 'Caches', 'aruna-desktop', 'blob')}`,
    ])
  })

  it('finds nothing to say about a Library left as it was', () => {
    mkdirSync(join(library, 'WebKit', 'aruna'), { recursive: true })
    expect(changed(traces(library), traces(library))).toEqual([])
  })

  it('gives the application a home of its own and fails the run that wrote outside it', async () => {
    const env: Record<string, string> = {}
    const run = isolate(env, library)
    expect(env.HOME).toBe(join(run.root, 'home'))
    expect(env.CFFIXED_USER_HOME).toBe(env.HOME)
    expect(env.TMPDIR).toBe(`${join(run.root, 'tmp')}/`)

    mkdirSync(join(library, 'WebKit', 'aruna-desktop'))
    await expect(finish(run, 'no-such-binary-of-aruna', library)).rejects.toThrow(
      `Aruna E2E wrote into the real Library:\nappeared: ${join(library, 'WebKit', 'aruna-desktop')}`,
    )
  })

  it('removes the home of the run when it ends', async () => {
    const temporary = mkdtempSync(join(tmpdir(), 'aruna-temporary-'))
    const run = isolate({}, library, temporary)
    mkdirSync(join(temporary, 'aruna-desktop', 'WebKit'), { recursive: true })
    await finish(run, 'no-such-binary-of-aruna', library)
    expect(existsSync(run.root)).toBe(false)
    expect(existsSync(join(temporary, 'aruna-desktop'))).toBe(false)
    rmSync(temporary, { recursive: true })
  })

  it('leaves WebKit’s temporary folder alone when it was there before the run', async () => {
    const temporary = mkdtempSync(join(tmpdir(), 'aruna-temporary-'))
    mkdirSync(join(temporary, 'aruna-desktop'))
    await finish(isolate({}, library, temporary), 'no-such-binary-of-aruna', library)
    expect(existsSync(join(temporary, 'aruna-desktop'))).toBe(true)
    rmSync(temporary, { recursive: true })
  })
})
