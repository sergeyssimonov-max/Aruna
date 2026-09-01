<svelte:options preserveWhitespace />

<script lang="ts">
  /**
   * One entry of the table's `<colgroup>`, which the stylesheet gives its
   * width.
   *
   * Rendered once at build time into `cli/src/generated/column_width.html`.
   * The crate repeats it once per column, from the same `COLUMNS` list that
   * feeds `ColumnHeading.svelte` and `LegendEntry.svelte`.
   */
  const {
    className = '',
  }: {
    /** The `class` the stylesheet matches on — `c-num`, `c-sig`, and so on. */
    className?: string
  } = $props()
</script>

<col class={className} />
