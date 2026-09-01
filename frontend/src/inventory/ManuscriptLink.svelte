<svelte:options preserveWhitespace />

<script lang="ts">
  /**
   * The title cell's contents when the manuscript has a file beside it.
   *
   * Rendered once at build time into `cli/src/generated/manuscript_link.html`.
   * The crate fills it and passes the result on as `ManuscriptRow.svelte`'s
   * `title`; where there is no file to link at, it passes the bare name
   * instead. The cell's text is the same either way, which is what the
   * document's own search reads.
   */
  const {
    href = '',
    title = '',
  }: {
    /** Where the manuscript's own file sits, relative to the document. */
    href?: string
    /** The name the manuscript is listed under. */
    title?: string
  } = $props()
</script>

<a {href} target="_blank" rel="noopener">{title}</a>
