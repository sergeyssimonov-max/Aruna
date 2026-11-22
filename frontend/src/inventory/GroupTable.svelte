<svelte:options preserveWhitespace />

<script lang="ts">
  const {
    label = '',
    colgroup = '',
    thead = '',
    rows = '',
  }: { label?: string; colgroup?: string; thead?: string; rows?: string } = $props()
</script>

<table aria-label={label}>
  <colgroup>{@html colgroup}</colgroup>
  <thead>
    <tr>{@html thead}</tr>
  </thead>
  <tbody>
    {@html rows}
  </tbody>
</table>
