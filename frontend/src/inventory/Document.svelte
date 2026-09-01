<svelte:options preserveWhitespace />

<script lang="ts">
  /**
   * The exported inventory, as one page.
   *
   * Rendered once at build time with placeholder props and written to
   * `cli/src/generated/document.html`, which the Rust crate compiles in and
   * fills at run time — see `build/inventory.ts`. Nothing here ever runs in a
   * browser: what the reader's browser runs is `filter.ts`, pasted into the
   * `<script>` element below.
   *
   * The props divide the way the substitution does. Text the reader sees is a
   * plain expression, which Svelte escapes; markup the Rust side assembles from
   * its own templates — the rows, the columns, the legend — arrives through
   * `{@html}`, already escaped by `escape_html` on the way in. Svelte will not
   * accept a text expression inside `<colgroup>`, `<thead>` or `<tbody>`
   * anyway: only `{@html}` may stand where a `<col>`, `<th>` or `<tr>` belongs.
   *
   * The doctype is written through `{@html}` for the same reason and not as
   * a matter of taste: Svelte's parser reads a literal `<!DOCTYPE html>` as an
   * element named `!DOCTYPE` with an attribute named `html`, and renders it
   * back out as `<!doctype html=""/>`. The crate expects the artifact to
   * begin at the doctype — see `DOCUMENT` in `cli/src/html.rs`.
   */
  const {
    style = '',
    script = '',
    source = '',
    authors = '',
    generated = '',
    manuscripts = '',
    groups = '',
    legend = '',
    colgroup = '',
    thead = '',
    rows = '',
  }: {
    /** The whole stylesheet, three sections joined in `style.rs`. */
    style?: string
    /** The client script, bundled from `main.ts` by this same build. */
    script?: string
    /** Where the corpus came from — a Zenodo record line. */
    source?: string
    /** Who is credited with the corpus itself, not with one manuscript. */
    authors?: string
    /** The `Generated:` line, or nothing when there is no timestamp to give. */
    generated?: string
    /** How many manuscripts the table holds. */
    manuscripts?: string
    /** How many CTH groups they fall into. */
    groups?: string
    /** One `<li>` per column, from the crate's single list of them. */
    legend?: string
    /** The `<col>` elements, same list. */
    colgroup?: string
    /** The `<th>` elements, same list. */
    thead?: string
    /** Every group heading and every manuscript, in order. */
    rows?: string
  } = $props()
</script>

{@html '<!DOCTYPE html>'}
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Thesaurus Linguarum Hethaeorum Digitalis</title>
    <svelte:element this={"style"}>{@html style}</svelte:element>
  </head>
  <body>
    <main>
      <h1>Thesaurus Linguarum Hethaeorum Digitalis</h1>
      <p class="meta">
        <span>Source: {source}</span>
        <span>Corpus authors: {authors}</span>
        {@html generated}
        <span class="counts"
          ><span class="count">Manuscripts: {manuscripts}</span><span class="count"
            >Groups (CTH): {groups}</span
          ></span
        >
      </p>
      <section class="legend" aria-label="Column legend">
        <p class="legend-title">Columns</p>
        <ul class="legend-list">
          {@html legend}
        </ul>
      </section>
      <div class="toolbar">
        <input
          type="search"
          id="q"
          aria-label="Поиск по описи"
          placeholder="Search CTH, siglum, lang, corpus, editor, year…"
          autocomplete="off"
          spellcheck="false"
        />
        <button type="button" id="fold-all" class="fold-all" aria-expanded="true"
          >Collapse fragments</button
        >
        <span class="hint" id="hint"></span>
      </div>
      <table id="inv">
        <colgroup>{@html colgroup}</colgroup>
        <thead>
          <tr>{@html thead}</tr>
        </thead>
        <tbody>
          {@html rows}
        </tbody>
      </table>
    </main>
    <svelte:element this={"script"}>{@html script}</svelte:element>
  </body>
</html>
