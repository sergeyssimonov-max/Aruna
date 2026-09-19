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
 *
 * **The scrape has to survive `rustfmt`.** The first version of this file
 * looked for `("code", Some(` on one line, and `rustfmt` had already broken the
 * `Network` arm over four — so the guard reported twenty-three codes, missed
 * the one that is hit most often, and said nothing. Newlines are allowed
 * between every token below for that reason, and the count is asserted against
 * the number of arms rather than against a floor picked by hand.
 */
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const at = (relative: string) => fileURLToPath(new URL(relative, import.meta.url))

/** Read once, at module level, the way the tests beside this one do. */
const RUST = readFileSync(at('../../cli/src/app.rs'), 'utf8')
const SHELL = readFileSync(at('../../src-tauri/src/lib.rs'), 'utf8')
const SVELTE = readFileSync(at('../src/App.svelte'), 'utf8')

const MATCH = 'let (code, phase, retryable) = match error {'
const TABLE = 'const FAILED: Record<string, string | undefined> = {'
const AFTER_TABLE = 'const DETAILED'

/** The codes `app::Failure::of` can produce, read out of its match arm. */
function coreCodes(): Set<string> {
  expect(RUST, `cli/src/app.rs no longer holds ${MATCH}`).toContain(MATCH)

  const start = RUST.indexOf(MATCH)
  const arm = RUST.slice(start, RUST.indexOf('\n        };', start))
  const codes = [...arm.matchAll(/\(\s*"([a-z_]+)"\s*,\s*(?:Some\s*\(|None)/g)].map(
    (match) => match[1],
  )

  // One per arm, and the arms are the lines that end in `=> (` or `=> {`
  // plus the ones written inline. Counted rather than floored: a code the
  // regular expression stops matching would otherwise vanish in silence.
  expect(codes.length, 'the match arm is no longer shaped the way this reads it').toBe(
    (arm.match(/\(\s*"[a-z_]+"\s*,/g) ?? []).length,
  )
  return new Set(codes)
}

/** The sentences the window has, read out of its table, keyed by code. */
function windowTable(): { codes: Set<string>; sentences: string[] } {
  expect(SVELTE, `src/App.svelte no longer holds ${TABLE}`).toContain(TABLE)

  const table = SVELTE.slice(SVELTE.indexOf(TABLE), SVELTE.indexOf(AFTER_TABLE))

  return {
    codes: new Set([...table.matchAll(/^\s{4}([a-z_]+):/gm)].map((match) => match[1])),
    sentences: [...table.matchAll(/'([^']{10,})'/g)].map((match) => match[1]),
  }
}

describe('the window has a Russian sentence for every failure the core sends', () => {
  it('cli/src/app.rs and src/App.svelte name the same codes', () => {
    const { codes } = windowTable()

    // `cancelled` is the one code whose sentence depends on the phase rather
    // than on the code, so it has a table of its own — and six tests of its
    // own in `App.test.ts`, which is where its absence would show.
    const missing = [...coreCodes()].filter((code) => code !== 'cancelled' && !codes.has(code))

    expect(missing, `these codes would reach the reader in English: ${missing.join(', ')}`).toEqual(
      [],
    )
  })

  /**
   * The shell has failures of its own — a run already going, a chosen folder
   * that is gone — and they never reach `Failure::of`, so the table above does
   * not hold them and the window shows their `message` through the fallback.
   * That is the right owner for them: the shell knows what it refused and why.
   * What it must not do is say it in English, and nothing said so until here.
   */
  it('the failures the shell raises itself are Russian too', () => {
    const raised = [...SHELL.matchAll(/BuildFailure::shell\(\s*"[a-z_]+",\s*"([^"]+)"/g)].map(
      (match) => match[1],
    )

    expect(raised.length, 'no shell-raised failures found — has the helper moved?').toBeGreaterThan(
      2,
    )
    for (const said of raised) {
      expect(said, `${said} is what the reader would see`).toMatch(/[А-Яа-я]/)
      expect(said, `${said} breaks the typography rules`).not.toMatch(/[ёЁ—]/)
    }
  })

  it('every sentence in the window is Russian', () => {
    const { codes, sentences } = windowTable()
    expect(sentences.length).toBe(codes.size)

    for (const sentence of sentences) {
      expect(sentence, `${sentence} has no Cyrillic in it`).toMatch(/[А-Яа-я]/)
      // «ё» is not used in this project, and «—» is not the dash it uses.
      expect(sentence, `${sentence} breaks the typography rules`).not.toMatch(/[ёЁ—]/)
    }
  })
})
