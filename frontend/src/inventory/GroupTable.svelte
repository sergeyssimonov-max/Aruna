<svelte:options preserveWhitespace />

<script lang="ts">
  const {
    label = '',
    colgroup = '',
    thead = '',
    bodies = '',
  }: { label?: string; colgroup?: string; thead?: string; bodies?: string } = $props()
</script>

<table aria-label={label}>
  <colgroup>{@html colgroup}</colgroup>
  <thead>
    <tr>{@html thead}</tr>
  </thead>
  {@html bodies}
</table>
