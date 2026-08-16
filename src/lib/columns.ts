/**
 * The inventory table's columns — one description, read by everything that
 * draws them.
 *
 * The legend above the table and the heading row are the same six columns in
 * the same order. Written out twice they were two places to forget, and the
 * failure is quiet: a table whose legend and headings disagree still renders.
 * The CLI's HTML output keeps the same list in `cli/src/html.rs`; the two
 * cannot import each other, but each is internally single.
 */

export type Column = {
  /** Grid class the stylesheet gives this column its width. */
  className: string;
  /** Text in the heading row. */
  head: string;
  /** What the legend says the column holds. */
  legend: string;
};

export const COLUMNS: readonly Column[] = [
  { className: "vl-c-num", head: "№", legend: "row number" },
  { className: "vl-c-sig", head: "Siglum", legend: "publication id (e.g. KBo 3.22)" },
  { className: "vl-c-lang", head: "Lang", legend: "languages, most-used first (Hit, Hur…)" },
  { className: "vl-c-corp", head: "Corpus", legend: "edition series (HFR, TLH, HAnn…)" },
  { className: "vl-c-ed", head: "Editor", legend: "transliteration / edition author" },
  { className: "vl-c-year", head: "Year", legend: "edition year" },
];
