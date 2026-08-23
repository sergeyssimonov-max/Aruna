/**
 * How the inventory's client script and stylesheet sections are built, in one
 * place.
 *
 * Two callers share it: `vite.inventory.config.ts`, which writes the artifacts
 * the Rust crate compiles in, and `tests/inventory-artifact.test.ts`, which
 * builds into a temporary directory and fails if what is committed is not
 * byte-for-byte what these sources produce. They have to be the same build or
 * the test proves nothing, so neither of them states these options itself.
 *
 * One `vite build` produces all four artifacts: the script is the entry, and a
 * plugin runs the three stylesheet sections after it. Separate builds rather
 * than one bundle on purpose — the three sections stay three files, because the
 * order they are emitted in *is* the cascade and that decision belongs to
 * `cli/src/style.rs`, which is the one place that makes it.
 */
import type { InlineConfig, Plugin } from 'vite'
import { build } from 'vite'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

/** The script the exported inventory carries. */
export const SCRIPT = 'inventory_filter.js'

/**
 * The three stylesheet sections, in the order `style.rs` emits them.
 *
 * `canonical` is what any document this program writes is before it adds
 * anything of its own; `screen` is the inventory's own rules; `print` is how it
 * prints, and the future PDF joins there.
 */
export const SECTIONS = ['canonical', 'screen', 'print'] as const

/** Every file the build writes, which is what the agreement test compares. */
export const ARTIFACTS = [SCRIPT, ...SECTIONS.map((name) => `${name}.css`)]

/** Where the committed artifacts live: `cli/src/generated/`, next to the crate. */
export const CRATE_OUT = fileURLToPath(new URL('../../cli/src/generated/', import.meta.url))

const ROOT = fileURLToPath(new URL('..', import.meta.url))
const source = (file: string) => fileURLToPath(new URL(`../src/inventory/${file}`, import.meta.url))

/**
 * The engine floor of the **exported document** — which is not the window's.
 *
 * `frontend/vite.config.ts` builds the desktop window for `safari16`, because
 * that is the WKWebView macOS 13 ships and never updates; the specification
 * fixes it. This artifact is a different promise: the inventory travels with
 * the corpus and is opened by a reader on a browser nobody here chose. Copying
 * the window's floor would make the corpus's reach a side effect of one
 * laptop's operating system.
 *
 * **Safari 13, and the number costs almost nothing.** Built at 11, 12, 13, 14
 * and 15 these sections come out byte-for-byte identical: they use nothing that
 * wants lowering. The floor decides exactly one declaration —
 * `-webkit-appearance` on the search field, which the hand-written stylesheet
 * carried and which a floor of 16 drops as redundant. Thirty-nine bytes to keep
 * the search box looking right on a browser from before March 2022.
 */
const DOCUMENT_TARGET = 'safari13'

/**
 * The same floor in Lightning CSS's encoding: major in the high sixteen bits.
 *
 * **Stating it is what makes it apply.** Vite resolves the transformer's
 * compilation targets to its own baseline — Chrome 111 and its contemporaries —
 * whenever `css.lightningcss.targets` is left unset, and `build.cssTarget`
 * reaches Lightning CSS only when it is *minifying*. This build does not
 * minify, so without the line below these sections would be compiled against a
 * floor higher than the one the window uses, let alone the one the corpus
 * needs. That was found by probing the output and confirmed afterwards in
 * Vite's own documentation; it is not a setting that can be dropped as
 * redundant.
 */
const DOCUMENT_FLOOR = 13 << 16

/**
 * Vite appends this to a stylesheet it has bundled. It is not ours, it means
 * nothing outside the bundler, and a document that carried it would be the one
 * comment in a stylesheet whose comments are all deliberately removed.
 */
const VITE_MARKER = /\/\*\$vite\$:\d+\*\/\s*$/

/**
 * Scaffolding Lightning CSS declares whenever it sees `color-scheme`, so that a
 * `light-dark()` value could be polyfilled on engines without it.
 *
 * The sections never call `light-dark()`, so the two properties are declared,
 * read by nothing, and would travel into every exported document — the only
 * lines in a stylesheet whose every comment was deliberately removed. They go
 * the same way the comments do.
 */
const LIGHTNINGCSS_SCAFFOLD = /^ *--lightningcss-(?:light|dark): *[^;]*;\n/gm

/**
 * Removing the scaffolding is only safe while nothing reads it.
 *
 * `light-dark()` compiles *into* those two properties, so a section that used
 * the function would render wrongly rather than fail — which is why this is
 * checked at build time and not left to a reviewer.
 */
function guardAgainstLightDark(name: string, css: string): void {
  if (css.includes('var(--lightningcss-')) {
    throw new Error(
      `${name}.css compiles to a light-dark() polyfill, whose declarations this ` +
        `build removes. Drop LIGHTNINGCSS_SCAFFOLD in build/inventory.ts first.`,
    )
  }
}

/** A file the CSS builds emit and nothing reads: lib mode always writes a chunk. */
const STUB = '_stub.js'

/**
 * Vite refuses a stylesheet as a library entry, so each section is reached
 * through a one-line module that imports it. The module is written into a
 * temporary directory rather than kept in the tree: it is punctuation the
 * bundler needs, not a source file anyone should find and wonder about. Its
 * path never reaches the output, so two builds still agree byte for byte.
 */
async function entryFor(name: string): Promise<{ dir: string; file: string }> {
  const dir = await mkdtemp(join(tmpdir(), `aruna-css-${name}-`))
  const file = join(dir, 'entry.js')
  await writeFile(file, `import ${JSON.stringify(source(`${name}.css`))}\n`)
  return { dir, file }
}

/**
 * One stylesheet section: lowered to the floor, comments dropped, rules left
 * alone.
 *
 * Not minified. The exported document is read by people who open its source —
 * that is why `style.rs` never minified either — and Lightning CSS lowers with
 * `targets` whether or not it is also asked to compress. Comments go because
 * they explain to a maintainer why a rule is written the way it is, which is
 * not a question the reader of a corpus has.
 */
function sectionBuild(name: string, entry: string, outDir: string): InlineConfig {
  return {
    root: ROOT,
    configFile: false,
    logLevel: 'warn',
    // The root's `public/` belongs to the desktop window; copying it into the
    // crate would put a favicon next to the artifacts.
    publicDir: false,
    css: {
      transformer: 'lightningcss',
      lightningcss: { targets: { safari: DOCUMENT_FLOOR } },
    },
    build: {
      outDir,
      emptyOutDir: false,
      target: DOCUMENT_TARGET,
      minify: false,
      cssMinify: false,
      cssCodeSplit: false,
      reportCompressedSize: false,
      lib: {
        entry,
        formats: ['es'],
        fileName: () => STUB,
        cssFileName: name,
      },
    },
  }
}

/** Builds the three sections after the script, and tidies up after them. */
function stylesheets(outDir: string): Plugin {
  return {
    name: 'aruna-inventory-stylesheets',
    async closeBundle() {
      try {
        await buildSections(outDir)
      } finally {
        // Lib mode always writes a chunk beside each stylesheet. It must not
        // survive a failed build either: this directory is the crate's.
        await rm(join(outDir, STUB), { force: true })
      }
    },
  }
}

/** The three sections, in order: built, checked, and freed of bundler leavings. */
async function buildSections(outDir: string): Promise<void> {
  for (const name of SECTIONS) {
    const { dir, file } = await entryFor(name)
    try {
      await build(sectionBuild(name, file, outDir))
    } finally {
      await rm(dir, { recursive: true, force: true })
    }
    const css = join(outDir, `${name}.css`)
    const text = await readFile(css, 'utf8')
    guardAgainstLightDark(name, text)
    await writeFile(css, text.replace(VITE_MARKER, '').replace(LIGHTNINGCSS_SCAFFOLD, ''))
  }
}

/**
 * The build, aimed at `outDir`.
 *
 * Everything that would make two builds differ is turned off. File names are
 * fixed rather than hashed, nothing is minified, and the script is an IIFE with
 * no exports — the same shape the hand-written one had, because it is pasted
 * inside a `<script>` element in a document that has no module loader and may
 * be opened from a `file://` URL.
 */
export function inventoryBuild(outDir: string): InlineConfig {
  return {
    root: ROOT,
    configFile: false,
    logLevel: 'warn',
    publicDir: false,
    plugins: [stylesheets(outDir)],
    build: {
      outDir,
      emptyOutDir: false,
      target: DOCUMENT_TARGET,
      minify: false,
      cssCodeSplit: false,
      reportCompressedSize: false,
      lib: {
        entry: source('main.ts'),
        formats: ['iife'],
        name: 'ArunaInventory',
        fileName: () => SCRIPT,
      },
    },
  }
}
