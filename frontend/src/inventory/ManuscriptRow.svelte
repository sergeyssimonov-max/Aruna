<svelte:options preserveWhitespace />

<script lang="ts">
  /**
   * One manuscript: the six cells, in the order the crate's `COLUMNS` names
   * them.
   *
   * Rendered once at build time into `cli/src/generated/manuscript_row.html`
   * and filled in per record at run time by `cli/src/html.rs`.
   *
   * Two props carry markup. `title` holds either a link or a bare name, and
   * which of the two is decided in the crate — see `ManuscriptLink.svelte`.
   * `editor` holds the name and, after it, a hidden span with the corpus's
   * other spellings of the same person, listed in the crate's
   * `presentation::EDITOR_ALIASES`.
   * The search in the exported inventory matches a row by its text, and hidden
   * text is still text, so `schwemer` reaches a row that prints only `DS`.
   * Everything else is text, so Svelte escapes it.
   */
  const {
    number = '',
    title = '',
    lang = '',
    corpus = '',
    editor = '',
    year = '',
  }: {
    /** The row's ordinal, counted across the whole table. */
    number?: string
    /** The manuscript's name — markup, because it may already be a link. */
    title?: string
    /** Languages, most-used first. */
    lang?: string
    /** The edition series the manuscript belongs to. */
    corpus?: string
    /**
     * Who transliterated or edited this one manuscript, and, out of sight, the
     * corpus's other spellings of that same person — decided in the crate.
     */
    editor?: string
    /** The year of the edition. */
    year?: string
  } = $props()
</script>

<tr>
  <td class="num">{number}</td>
  <td>{@html title}</td>
  <td>{lang}</td>
  <td>{corpus}</td>
  <td>{@html editor}</td>
  <td class="year">{year}</td>
</tr>
