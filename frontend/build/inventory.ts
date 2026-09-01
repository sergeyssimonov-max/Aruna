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
 * One `vite build` produces every artifact: the script is the entry, and two
 * plugins run after it — one for the three stylesheet sections, one for the
 * document and the fragments of markup. Separate builds rather than one bundle
 * on purpose — the three sections stay three files, because the order they are
 * emitted in *is* the cascade and that decision belongs to `cli/src/style.rs`,
 * which is the one place that makes it.
 *
 * **What the markup step is.** `src/inventory/*.svelte` is rendered once here,
 * by `render()` from `svelte/server`, with every prop set to a placeholder; the
 * HTML that comes out is the shape of the exported inventory with holes in it,
 * and `cli/src/html.rs` fills the holes when a corpus is exported. Nothing in
 * those components is ever mounted or hydrated: this is a template engine whose
 * templates happen to be type-checked, formatted and linted like the rest of
 * the frontend, which is the whole reason for authoring them in Svelte.
 */
import type { InlineConfig, Plugin } from 'vite'
import type { Component } from 'svelte'
import { build } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { render } from 'svelte/server'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

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

/**
 * A placeholder the Rust side finds and replaces when it exports a corpus.
 *
 * The name is the component's own prop name, upper-cased, so that a prop and
 * the hole it leaves in the artifact cannot be given different names. What the
 * hole is filled with is `cli/src/html.rs`'s business; that the hole is there
 * at all is checked below, before the artifact is written.
 */
const placeholder = (prop: string) => `@@${prop.toUpperCase()}@@`

/** Every named prop rendered as its own placeholder. */
const placeholders = (...props: readonly string[]): Record<string, string> =>
  Object.fromEntries(props.map((prop) => [prop, placeholder(prop)]))

/**
 * The stylesheet and the script are thousands of lines each.
 *
 * A document that opened either of them halfway along the line that carries the
 * `<style>` tag would be unreadable in a way the hand-written one never was, so
 * the break before and the indent after belong to the placeholder's value. That
 * keeps the shape of the page a decision of this build rather than of Rust,
 * which only ever hands over the text.
 */
const inOwnBlock = (prop: string) => `\n${placeholder(prop)}\n    `

/**
 * What is rendered, into what, and with which props.
 *
 * `Document.svelte` is the whole page and everything else is a fragment the
 * crate repeats — a row per manuscript, a heading per CTH group, a `<col>`,
 * `<th>` and legend line per column. They are separate components rather than
 * `{#each}` blocks inside the document because how many of each there are is
 * only known when a corpus is exported, and because the crate's `COLUMNS` is
 * the one declaration of the columns: a Svelte template that listed them again
 * would be the second, and the two would drift.
 */
const MARKUP: readonly {
  /** The component, in `src/inventory/`. */
  component: string
  /** The artifact it becomes, in `cli/src/generated/`. */
  artifact: string
  /** What it is rendered with — a placeholder per prop. */
  props: Record<string, string>
}[] = [
  {
    component: 'Document.svelte',
    artifact: 'document.html',
    props: {
      ...placeholders(
        'source',
        'authors',
        'generated',
        'manuscripts',
        'groups',
        'legend',
        'colgroup',
        'thead',
        'rows',
      ),
      style: inOwnBlock('style'),
      script: inOwnBlock('script'),
    },
  },
  {
    component: 'GroupHeading.svelte',
    artifact: 'group_heading.html',
    props: placeholders('span', 'label', 'count'),
  },
  {
    component: 'ManuscriptRow.svelte',
    artifact: 'manuscript_row.html',
    props: placeholders('number', 'title', 'lang', 'corpus', 'editor', 'year'),
  },
  {
    component: 'ManuscriptLink.svelte',
    artifact: 'manuscript_link.html',
    props: placeholders('href', 'title'),
  },
  {
    component: 'ColumnWidth.svelte',
    artifact: 'column_width.html',
    props: placeholders('className'),
  },
  {
    component: 'ColumnHeading.svelte',
    artifact: 'column_heading.html',
    props: placeholders('head'),
  },
  {
    component: 'LegendEntry.svelte',
    artifact: 'legend_entry.html',
    props: placeholders('head', 'legend'),
  },
  {
    component: 'GeneratedLine.svelte',
    artifact: 'generated_line.html',
    props: placeholders('generated'),
  },
]

/** Every file the build writes, which is what the agreement test compares. */
export const ARTIFACTS = [
  SCRIPT,
  ...SECTIONS.map((name) => `${name}.css`),
  ...MARKUP.map(({ artifact }) => artifact),
]

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
 * The markers Svelte leaves for a client that is going to hydrate the page.
 *
 * `render()` brackets a component with `<!--[-->` and `<!--]-->` and fences each
 * `{@html}`, so that the runtime can find its own work later. Nothing hydrates
 * this document — it carries `filter.ts` and no Svelte at all — and comments in
 * it were deliberately taken out along with the stylesheet's, so they go the
 * same way the `$vite$` marker above does.
 *
 * **Every comment, not a list of the ones seen so far.** A component's own HTML
 * comments never reach `render()`'s output — the compiler drops them unless
 * `preserveComments` is set, and it is not — so any comment here is the
 * renderer's. Matching them by shape was tried and is wrong: the fence around
 * `{@html}` came out `<!---->` from `pnpm build:inventory` and `<!--12ftosl-->`
 * from the same build under Vitest, and an artifact that depends on which
 * command asked for it is not one the agreement test can compare.
 */
const HYDRATION_MARKERS = /<!--[\s\S]*?-->/g

/** The chunk an entry produces: the artifact's name without its extension. */
const chunkName = (artifact: string) => artifact.replace(/\.html$/, '')

/**
 * The components, compiled for the server and left as modules to be rendered.
 *
 * Not the document's engine floor and not the window's: this output is imported
 * by the Node that runs the build and never reaches a browser, so it is built
 * for the Node that is running. What travels to the corpus is the HTML these
 * modules produce.
 *
 * **`mode` and `dev` are stated rather than inherited, and that is the whole
 * reason the agreement test works.** Vite takes the mode from whoever is
 * running it: `pnpm build:inventory` builds in production, and
 * `tests/inventory-artifact.test.ts` runs under Vitest, which does not. Left to
 * itself the second one compiles the components in development mode, where the
 * server runtime keeps a validation context the components are not rendered
 * inside — it fails on the first element with `Cannot read properties of null`.
 * Even had it rendered, an artifact that depends on who asked for it is not an
 * artifact anyone can check.
 */
function markupBuild(outDir: string): InlineConfig {
  return {
    root: ROOT,
    configFile: false,
    logLevel: 'warn',
    mode: 'production',
    publicDir: false,
    plugins: [svelte({ configFile: false, compilerOptions: { css: 'external', dev: false } })],
    build: {
      ssr: true,
      outDir,
      emptyOutDir: false,
      minify: false,
      reportCompressedSize: false,
      rollupOptions: {
        input: Object.fromEntries(
          MARKUP.map(({ artifact, component }) => [chunkName(artifact), source(component)]),
        ),
        output: { entryFileNames: '[name].js', format: 'esm' },
      },
    },
  }
}

/**
 * An artifact is only worth committing if it still has its holes.
 *
 * A renamed prop would otherwise produce a perfectly valid document with a
 * sentence missing from it, discovered by a reader of the corpus rather than
 * here. The second check is the one that catches a `{#if}` or an `{#each}`
 * added to a component later: either would leave a marker behind.
 */
function guard(artifact: string, props: Record<string, string>, html: string): void {
  for (const prop of Object.keys(props)) {
    if (!html.includes(placeholder(prop))) {
      throw new Error(`${artifact} has no ${placeholder(prop)}: the ${prop} prop reached nothing.`)
    }
  }
  const comment = html.indexOf('<!--')
  if (comment >= 0) {
    throw new Error(
      `${artifact} carries ${JSON.stringify(html.slice(comment, comment + 40))}; ` +
        `HYDRATION_MARKERS in build/inventory.ts did not reach it.`,
    )
  }
}

/** The document and its fragments: built, rendered, checked, written. */
async function buildMarkup(outDir: string): Promise<void> {
  const modules = await mkdtemp(join(tmpdir(), 'aruna-markup-'))
  try {
    await build(markupBuild(modules))
    for (const { artifact, props } of MARKUP) {
      const url = pathToFileURL(join(modules, `${chunkName(artifact)}.js`)).href
      const module = (await import(url)) as { default: Component<Record<string, string>> }
      const html = render(module.default, { props }).body.replace(HYDRATION_MARKERS, '').trim()
      guard(artifact, props, html)
      await writeFile(join(outDir, artifact), `${html}\n`)
    }
  } finally {
    await rm(modules, { recursive: true, force: true })
  }
}

/** Renders the markup after the script, the way the stylesheets are rendered. */
function markup(outDir: string): Plugin {
  return {
    name: 'aruna-inventory-markup',
    async closeBundle() {
      await buildMarkup(outDir)
    },
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
    plugins: [stylesheets(outDir), markup(outDir)],
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
