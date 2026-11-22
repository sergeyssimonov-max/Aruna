<svelte:options preserveWhitespace />

<script lang="ts">
  /**
   * One column heading.
   *
   * Rendered once at build time into `cli/src/generated/column_heading.html`
   * and repeated by the crate, once per entry of its `COLUMNS` list.
   */
  const {
    head = '',
    className = '',
  }: {
    /** What the column is called. */
    head?: string
    className?: string
  } = $props()
</script>

<th scope="col" class={className}>{head}</th>
