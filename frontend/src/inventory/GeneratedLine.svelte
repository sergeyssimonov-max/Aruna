<svelte:options preserveWhitespace />

<script lang="ts">
  /**
   * The timestamp line of the summary block.
   *
   * Rendered once at build time into `cli/src/generated/generated_line.html`.
   * The crate leaves it out when it has no timestamp to give: a package is
   * rebuilt from the same archive and has to come out the same every time, and
   * a clock reading in the document would be the one thing that never did.
   */
  const {
    generated = '',
  }: {
    /** The date and time, already formatted by the crate. */
    generated?: string
  } = $props()
</script>

<span>Generated: {generated}</span>
