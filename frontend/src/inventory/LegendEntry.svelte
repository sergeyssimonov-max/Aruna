<svelte:options preserveWhitespace />

<script lang="ts">
  /**
   * One line of the legend above the table: a column, and what it holds.
   *
   * Rendered once at build time into `cli/src/generated/legend_entry.html`
   * and repeated by the crate, once per entry of its `COLUMNS` list.
   */
  const {
    head = '',
    legend = '',
  }: {
    /** The column's heading, repeated here so the two can be matched up. */
    head?: string
    /** What the column holds, spelled out. */
    legend?: string
  } = $props()
</script>

<li><span class="k">{head}</span><span class="d">{legend}</span></li>
