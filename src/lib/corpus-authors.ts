/**
 * The people credited with the corpus, and the city each worked in.
 *
 * Read from `metadata.creators` of Zenodo record 20328284, which gives an
 * institution rather than a city — `University of Würzburg` for Müller and
 * Schwemer, `Johannes Gutenberg University Mainz` for Prechel, `Philipps
 * University of Marburg` for Rieken. The city is written out rather than cut
 * from the institution's name at run time, because a rule that finds the place
 * inside `Johannes Gutenberg University Mainz` works on these four by luck.
 *
 * The CLI carries the same list in `CORPUS_AUTHORS` (`cli/src/lib.rs`) for the
 * standalone HTML it writes, and the two cannot import each other —
 * `corpus-authors-agreement.test.ts` is what keeps them saying the same thing.
 *
 * Not fetched from Zenodo: this page loads a committed catalog and never talks
 * to the repository, so a credit that arrived over the network would be missing
 * exactly here.
 */
export const CORPUS_AUTHORS: ReadonlyArray<{ name: string; city: string }> = [
  { name: "Gerfrid Müller", city: "Würzburg" },
  { name: "Doris Prechel", city: "Mainz" },
  { name: "Elisabeth Rieken", city: "Marburg" },
  { name: "Daniel Schwemer", city: "Würzburg" },
];

/** The one line both inventories print. */
export function corpusAuthorsLine(): string {
  return CORPUS_AUTHORS.map(({ name, city }) => `${name} (${city})`).join(", ");
}
