<svelte:options preserveWhitespace />

<script lang="ts">
  /**
   * A CTH section heading, and the control that folds its manuscripts away.
   *
   * One of the fragments `cli/src/html.rs` repeats: rendered once at build time
   * with placeholder props, written to `cli/src/generated/group_heading.html`,
   * and filled in per group at run time. How many headings there are is the
   * crate's business, so there is no `{#each}` here and nothing to repeat it
   * with.
   *
   * The whole heading is a `<button>` rather than a row with a click handler,
   * so the group can be folded from the keyboard and a screen reader is told
   * what the control does and what state it is in. `aria-expanded` starts
   * `true` because a document opened without JavaScript shows everything.
   */
  const {
    span = 0,
    label = '',
    count = '',
  }: {
    /**
     * How many columns the cell spans — the crate's `COLUMNS` list decides.
     *
     * The only prop here that is not a string, because `colspan` is typed as
     * a number and a string is refused. It makes no difference to the
     * artifact: what the build hands in is the placeholder, like everywhere
     * else, and the crate writes a count into it.
     */
    span?: number
    /** The CTH label the group is gathered under. Text, never a link. */
    label?: string
    /** How many manuscripts stand under this heading. */
    count?: string
  } = $props()
</script>

<tr class="group">
  <td colspan={span}
    ><button type="button" class="group-toggle" aria-expanded="true"
      ><span class="chevron" aria-hidden="true"></span><span class="group-label">{label}</span><span
        class="group-count">{count}</span
      ></button
    ></td
  >
</tr>
