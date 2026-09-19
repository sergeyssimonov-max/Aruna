/**
 * Every failure code the core can send has a Russian sentence in the window.
 *
 * The core writes one `message` per failure and writes it in English: the
 * console front end prints it beside English causes, and whoever reads it there
 * is building the program from source. The window is Russian throughout, and
 * the person in front of it sees nothing but the window — so the sentences live
 * in `App.svelte`, one per code, with the core's message left as the fallback
 * for a code the window has never heard of.
 *
 * That fallback is what this file guards. It is deliberate and it is also
 * silent: a code added to `Failure::of` tomorrow would land on the screen in
 * English and nothing would say so. This test reads both sides and refuses the
 * pair when they disagree — the same trick as `spec-guard`, applied across the
 * language boundary rather than across a document.
 *
 * Read from the sources rather than from a list kept here, because a list kept
 * here is a third place to forget.
 */
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const at = (relative: string) => fileURLToPath(new URL(relative, import.meta.url))

/** The codes `app::Failure::of` can produce, read out of the match arm. */
function coreCodes(): string[] {
  const rust = readFileSync(at('../../cli/src/app.rs'), 'utf8')
  const arm = rust.slice(rust.indexOf('let (code, phase, retryable) = match error {'))
  const codes = [...arm.matchAll(/\("([a-z_]+)",\s*(?:Some\(|None)/g)].map((match) => match[1])

  expect(
    codes.length,
    'no codes found in cli/src/app.rs — has the match arm moved?',
  ).toBeGreaterThan(10)
  return [...new Set(codes)]
}

/** The codes the window has a sentence for, read out of its two tables. */
function windowCodes(): { failed: Set<string>; window: string } {
  const svelte = readFileSync(at('../src/App.svelte'), 'utf8')
  const table = svelte.slice(
    svelte.indexOf('const FAILED: Record<string, string | undefined> = {'),
    svelte.indexOf('const DETAILED'),
  )

  expect(table, 'the FAILED table is no longer where this test looks').not.toHaveLength(0)
  return {
    failed: new Set([...table.matchAll(/^\s{4}([a-z_]+):/gm)].map((match) => match[1])),
    window: svelte,
  }
}

describe('the window has a Russian sentence for every failure the core sends', () => {
  it('cli/src/app.rs and src/App.svelte name the same codes', () => {
    const { failed, window } = windowCodes()

    // `cancelled` is the one code whose sentence depends on the phase rather
    // than on the code, so it has a table of its own.
    const cancelled = window.includes('const CANCELLED: Record<string, string | undefined> = {')
    expect(cancelled, 'the cancellation table is gone').toBe(true)

    const missing = coreCodes().filter((code) => code !== 'cancelled' && !failed.has(code))

    expect(missing, `these codes would reach the reader in English: ${missing.join(', ')}`).toEqual(
      [],
    )
  })

  it('every sentence in the window is Russian', () => {
    const { failed, window } = windowCodes()
    const table = window.slice(
      window.indexOf('const FAILED: Record<string, string | undefined> = {'),
      window.indexOf('const DETAILED'),
    )

    const sentences = [...table.matchAll(/'([^']{10,})'/g)].map((match) => match[1])
    expect(sentences.length).toBe(failed.size)

    for (const sentence of sentences) {
      expect(sentence, `${sentence} has no Cyrillic in it`).toMatch(/[А-Яа-я]/)
      // «ё» is not used in this project, and «—» is not the dash it uses.
      expect(sentence, `${sentence} breaks the typography rules`).not.toMatch(/[ёЁ—]/)
    }
  })
})
