/**
 * The committed build products are what these sources produce.
 *
 * Everything in `cli/src/generated/` is built here and compiled into the Rust
 * binary with `include_str!` — the client script and the three stylesheet
 * sections — and all four are committed rather than built by `build.rs` for one
 * reason: `cargo build` must never need Node. That is the
 * premise of the `.app` and the DMG, and it is worth the one thing a committed
 * build product costs — the chance of it going stale, or of someone editing
 * the artifact instead of the source. This is what makes that chance a failing
 * test rather than a mystery in an exported document.
 *
 * It builds with the very options `pnpm build:inventory` uses, because both
 * take them from `build/inventory.ts`. A test that stated its own would be
 * comparing against something the repository never produces.
 */
import { describe, expect, it } from 'vitest'
import { build } from 'vite'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { ARTIFACTS, CRATE_OUT, SCRIPT, inventoryBuild } from '../build/inventory.ts'

/** Every artifact as this working tree would build it, into a directory of its own. */
async function built(): Promise<Record<string, string>> {
  const out = await mkdtemp(join(tmpdir(), 'aruna-inventory-'))
  try {
    await build(inventoryBuild(out))
    return await read(out)
  } finally {
    await rm(out, { recursive: true, force: true })
  }
}

async function read(dir: string): Promise<Record<string, string>> {
  const files: Record<string, string> = {}
  for (const name of ARTIFACTS) files[name] = await readFile(join(dir, name), 'utf8')
  return files
}

const committed = () => read(CRATE_OUT)

describe('the artifacts the crate compiles in', () => {
  it('are byte-for-byte what the sources build', { timeout: 60_000 }, async () => {
    expect(await built()).toEqual(await committed())
  })

  /**
   * Two builds of one tree agree, so the artifacts can be compared at all —
   * and so `cli/tests/reliability.rs`, which requires two exports of one
   * archive to be byte-identical, is not undone by what is inside them.
   * A bundler that stamps a hash, a date or a variable chunk order into its
   * output is what would do it.
   */
  it('build the same twice', { timeout: 60_000 }, async () => {
    expect(await built()).toEqual(await built())
  })

  /**
   * The bundler's own leavings do not reach the corpus: the `$vite$` marker it
   * appends to a bundled stylesheet, the chunk lib mode writes beside one, and
   * the `light-dark()` scaffolding Lightning CSS declares for `color-scheme`.
   * Each is removed in `build/inventory.ts`, and each would otherwise appear in
   * a document whose every comment was deliberately taken out.
   */
  it('carry nothing the bundler left behind', async () => {
    const files = await committed()
    for (const [name, text] of Object.entries(files)) {
      expect(text, `${name} carries a bundler marker`).not.toMatch(/\$vite\$|--lightningcss-/)
    }
    expect(Object.keys(files)).not.toContain('_stub.js')
  })

  /**
   * The stylesheet sections stay three files because the order they are emitted
   * in *is* the cascade, and `cli/src/style.rs` is the one place that decides
   * it. Bundling them into one here would move that decision into the build,
   * where the reason for it is not written down.
   */
  it('keep the three stylesheet sections apart', async () => {
    const files = await committed()
    expect(files['canonical.css']).toContain(':root')
    expect(files['print.css']).toContain('@media print')
    expect(files['screen.css']).not.toContain('@media print')
    expect(files['screen.css']).not.toContain(':root')
  })

  /**
   * **The CTH heading has to survive printing, and it nearly did not.**
   *
   * The group's name is markup inside the button that folds the group
   * (`<span class="group-label">` inside `.group-toggle`, written by
   * `cli/src/html.rs`), so the print rule that hid the control as a
   * screen-only affordance hid every «CTH 5» with it: 663 groups of rows on
   * paper with nothing saying which group any row belonged to. What must go is
   * the chevron — a folded/open state paper does not have — and what must stay
   * is the heading.
   */
  it('keeps the CTH group headings on paper', async () => {
    const print = (await committed())['print.css']
    expect(print, 'the print rules hide the group control whole again').not.toMatch(
      /\.toolbar,\s*\.group-toggle\s*\{[^}]*display:\s*none/,
    )
    expect(print).toMatch(/\.group-toggle\s*\{[^}]*display:\s*inline-flex/)
    expect(print).toMatch(/\.group-toggle\s+\.chevron\s*\{[^}]*display:\s*none/)
  })

  /**
   * The shape the document needs, rather than the shape a module needs: it is
   * pasted inside a `<script>` element in a file that may be opened from a
   * `file://` URL, with no module loader and no network.
   */
  it('has a self-contained script and not a module', async () => {
    const js = (await committed())[SCRIPT]
    expect(js.startsWith('(function() {')).toBe(true)
    expect(js).not.toMatch(/\bimport\s|\bexport\s|import\.meta/)
    // `</script` anywhere in it would close the element early, whatever it
    // meant to the parser reading the JavaScript.
    expect(js).not.toContain('</script')
  })
})
