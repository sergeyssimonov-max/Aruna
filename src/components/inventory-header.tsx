import { COLUMNS } from "@/lib/columns";

/** Title, provenance, and the counts — what this page is and how much is in it. */
export function InventoryHeader({
  source,
  manuscripts,
  groups,
  matches,
}: {
  source: string;
  manuscripts: number;
  groups: number;
  /** Shown only while a search is running; `pending` marks a stale count. */
  matches: { count: number; pending: boolean } | null;
}) {
  return (
    <header className="mb-6 sm:mb-8">
      <div className="mb-3 min-w-0">
        <h1 className="text-[1.25rem] font-semibold leading-snug tracking-tight sm:text-[1.35rem]">
          Thesaurus Linguarum Hethaeorum Digitalis
        </h1>
        <p className="mt-1 text-[0.8125rem] leading-relaxed text-[#666]">{source}</p>
      </div>
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-[0.8125rem] text-[#666]">
        <Count label="Manuscripts" value={manuscripts} />
        <Count label="CTH groups" value={groups} />
        {matches && (
          <Count label="Match" value={matches.count}>
            {matches.pending ? <span className="ml-1 text-[#aaa]">…</span> : null}
          </Count>
        )}
      </div>
    </header>
  );
}

function Count({
  label,
  value,
  children,
}: {
  label: string;
  value: number;
  children?: React.ReactNode;
}) {
  return (
    <span>
      {label}:{" "}
      <strong className="font-medium tabular-nums text-[#1a1a1a]">{value.toLocaleString()}</strong>
      {children}
    </span>
  );
}

/** What each column of the table holds. */
export function ColumnLegend() {
  return (
    <section className="col-legend mb-4" aria-label="Column legend">
      <p className="col-legend-title">Columns</p>
      <ul className="col-legend-list">
        {COLUMNS.map((column) => (
          <li key={column.className}>
            <span className="col-legend-key">{column.head}</span>
            <span className="col-legend-def">{column.legend}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
