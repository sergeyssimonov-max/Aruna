<svelte:options preserveWhitespace />

<script lang="ts">
  const { rows = '', headings = '' }: { rows?: string; headings?: string } = $props()
</script>

<tbody>
  {@html rows}
  <tr class="print-columns" hidden>{@html headings}</tr>
</tbody>
