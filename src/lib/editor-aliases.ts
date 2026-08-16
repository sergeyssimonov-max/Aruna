/**
 * Spellings the corpus uses for one and the same editor.
 *
 * TLHdig records who made an edition in whatever form the file's author typed:
 * initials in most documents, a full name in the newer ones, and occasionally
 * both for the same person. A search for a surname therefore misses every row
 * that carries only initials — `schwemer` finds 84 manuscripts and not the 7
 * that say `DS`.
 *
 * This table is used **only when building the search index**. The table on the
 * page keeps whatever the document says, because the catalog is a transcript of
 * the archive and not an interpretation of it; what an alias changes is which
 * rows a query reaches. A wrong entry here therefore costs an unexpected row in
 * a result list, not a manuscript credited to someone who did not edit it.
 *
 * Entries are admitted on evidence from the corpus itself, not on plausibility:
 * the short form's letters must be the initials of the full name, **and** the
 * two spellings must appear together in documents — one person's initials in
 * one role and their name in another. `JB` and `James Burgin` are deliberately
 * absent: the initials match, but the two never occur in the same document, so
 * the identification would be an assumption. Initials with no full name
 * anywhere in the corpus — `LS`, `TS`, `MP` and twenty more, some in thousands
 * of manuscripts — cannot be resolved from the archive at all, and would need
 * TLHdig's own list of contributors.
 */

export type EditorAlias = {
  /** Every spelling of this person found in the corpus. */
  spellings: string[];
  /** Why these are one person, in terms of what the archive shows. */
  evidence: string;
};

export const EDITOR_ALIASES: readonly EditorAlias[] = [
  {
    spellings: ["DS", "ds", "Daniel Schwemer"],
    evidence:
      "initials match; the initials and the name appear together in 40 documents (34 as DS, 6 as ds)",
  },
  {
    spellings: ["FF", "Francesco Fuscagni"],
    evidence: "initials match; both spellings appear together in 15 documents",
  },
];

/** Lowercased spelling groups, built once. */
const GROUPS: string[][] = EDITOR_ALIASES.map((alias) =>
  alias.spellings.map((s) => s.toLowerCase()),
);

/**
 * The text a query should match for `editor`: what the document says, plus the
 * other spellings of the same person.
 *
 * Returned as one newline-joined string because that is how the index stores a
 * pooled value — the matcher is a substring test, so appending a spelling makes
 * the person findable under it without spending a second pool entry on them.
 */
export function searchableEditor(editor: string): string {
  const key = editor.trim().toLowerCase();
  const group = GROUPS.find((spellings) => spellings.includes(key));
  if (!group) return editor;
  // The document's own spelling first: it is the one the row displays.
  return [editor, ...group.filter((s) => s !== key)].join("\n");
}
