import { COLUMNS } from "@/lib/columns";
import type { FallbackReason } from "@/lib/search-protocol";

/** Title, provenance, and the counts — what this page is and how much is in it. */
export function InventoryHeader({
  source,
  manuscripts,
  groups,
  matches,
  fallbackReason,
}: {
  source: string;
  manuscripts: number;
  groups: number;
  /** Shown only while a search is running; `pending` marks a stale count. */
  matches: { count: number; pending: boolean } | null;
  /** Set when search fell back to the slower engine — see `SearchEngineNote`. */
  fallbackReason: FallbackReason | null;
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
        {fallbackReason && <SearchEngineNote reason={fallbackReason} />}
      </div>
    </header>
  );
}

/**
 * Said out loud only when it is true — and only about what actually happened.
 *
 * Results stay correct and the page keeps working when the binary index is out
 * of the picture, which is why the fallback was worth doing silently; silence
 * only meant nobody would learn the fast path had been off. That argument dies
 * the moment the explanation is wrong, so the reason comes from the worker
 * rather than from an assumption about which limit was hit.
 */
const EXPLANATION: Record<FallbackReason, string> = {
  unsupported:
    "This inventory does not fit the compact search index, so search is scanning the table directly. Results are the same; a long query takes a little longer.",
  unavailable:
    "The compact search index could not be loaded, so search is scanning the table directly. Results are the same; a long query takes a little longer.",
  trapped:
    "The compact search index stopped responding and was dropped for this session, so search is scanning the table directly. Results are the same; a long query takes a little longer.",
};

function SearchEngineNote({ reason }: { reason: FallbackReason }) {
  return (
    <span className="text-[#999]" title={EXPLANATION[reason]}>
      Search: fallback engine
    </span>
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
