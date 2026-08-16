import { createFileRoute } from "@tanstack/react-router";
import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { ColumnLegend, InventoryHeader } from "@/components/inventory-header";
import { InventoryTable } from "@/components/inventory-table";
import { SearchBar } from "@/components/search-bar";
import { applyMatches, type Group } from "@/lib/inventory";
import { useInventory } from "@/lib/use-inventory";
import { useScrollWindow } from "@/lib/use-scroll-window";
import { useSearchWorker } from "@/lib/use-search-worker";
import { buildLayout, countItems, visibleRows } from "@/lib/virtual-list";

/** How long a search may take before the page admits it is still working. */
const SLOW_SEARCH_MS = 200;

export const Route = createFileRoute("/")({
  component: InventoryPage,
  head: () => ({
    meta: [{ title: "Thesaurus Linguarum Hethaeorum Digitalis" }],
  }),
});

/**
 * The page: load the inventory, search it in a worker, and render the slice of
 * it that is on screen.
 *
 * Everything below is wiring — the loading lives in `useInventory`, the search
 * in `useSearchWorker`, the scroll arithmetic in `useScrollWindow` and
 * `virtual-list`, and the markup in `@/components`. What is left here is the
 * order those depend on each other in, which is the part that is specific to
 * this page.
 */
function InventoryPage() {
  const { data, arun, error: loadError } = useInventory();
  const [query, setQuery] = useState("");
  const [openAll, setOpenAll] = useState(true);
  const deferredQuery = useDeferredValue(query);

  const frameRef = useRef<HTMLDivElement>(null);
  const scrollerRef = useRef<HTMLDivElement>(null);

  const {
    status: workerStatus,
    fallbackReason: searchFallback,
    error: workerError,
    matches,
    matchesQuery,
    search,
  } = useSearchWorker(arun);

  // The worker normalises the query before searching; normalise the same way
  // here so a result can be matched to the query on screen.
  const normalizedQuery = deferredQuery.trim().toLowerCase();
  // Results for an earlier query are not results for this one.
  const currentMatches = matchesQuery === normalizedQuery ? matches : null;

  useEffect(() => {
    search(deferredQuery);
  }, [deferredQuery, search, workerStatus]);

  /** The groups this query calls for, or null while its result is in flight. */
  const answered: Group[] | null = useMemo(() => {
    if (!data) return null;
    if (!normalizedQuery) return data.groups;
    if (!currentMatches) return null;
    return applyMatches(data, currentMatches);
  }, [data, normalizedQuery, currentMatches]);

  // What is on screen. While a query is in flight the previous rows stay,
  // dimmed, rather than the table emptying: a keystroke used to blank it and
  // refill it a moment later, so typing a word flashed the list eight times.
  //
  // The rows shown are the previous query's, which is the thing an earlier fix
  // set out to stop — but the fault there was that stale hits were presented as
  // current. They are marked pending now, in the header and by the dimming, so
  // the page says "not updated yet" instead of either lying or blinking.
  const [shown, setShown] = useState<Group[]>([]);
  useEffect(() => {
    if (answered) setShown(answered);
  }, [answered]);
  const filtered: Group[] = answered ?? shown;

  const layout = useMemo(() => buildLayout(filtered, openAll), [filtered, openAll]);

  const { range, resetWindow } = useScrollWindow(scrollerRef, frameRef, data !== null);

  // A different set of groups is a different list: start it at the top.
  useEffect(() => resetWindow(), [filtered, openAll, resetWindow]);

  const rows = useMemo(
    () => visibleRows(filtered, layout, openAll, range.y0, range.y1),
    [filtered, layout, openAll, range.y0, range.y1],
  );

  const visibleCount = useMemo(() => {
    if (openAll) return layout.itemBase[layout.groupCount]!;
    return countItems(filtered);
  }, [filtered, layout, openAll]);

  const onQuery = useCallback((v: string) => setQuery(v), []);
  const onToggleOpenAll = useCallback(() => setOpenAll((v) => !v), []);

  const searching = normalizedQuery !== "";
  // Pending until a result for *this* query is in hand — the previous query's
  // matches are never null, so keying off that alone kept the indicator dark
  // through every re-search.
  const searchPending = searching && (workerStatus !== "ready" || currentMatches === null);

  // Shown only once the wait is long enough to notice. The worker usually
  // answers within a frame or two, and an indicator that appears and vanishes
  // on every keystroke is the same churn as the blanking it replaced.
  const [waiting, setWaiting] = useState(false);
  useEffect(() => {
    if (!searchPending) {
      setWaiting(false);
      return;
    }
    const timer = setTimeout(() => setWaiting(true), SLOW_SEARCH_MS);
    return () => clearTimeout(timer);
  }, [searchPending]);

  const error = loadError || workerError;
  if (error) return <Notice tone="error">{error}</Notice>;
  if (!data) return <Notice tone="muted">Loading inventory…</Notice>;

  return (
    <div className="min-h-screen bg-[#fafafa] text-[#1a1a1a]">
      <div className="mx-auto max-w-4xl px-4 pt-6 pb-10 sm:px-6">
        <InventoryHeader
          source={data.source}
          manuscripts={data.manuscripts}
          groups={data.groups.length}
          matches={searching ? { count: visibleCount, pending: waiting } : null}
          fallbackReason={searchFallback}
        />

        <ColumnLegend />

        <SearchBar
          query={query}
          onQuery={onQuery}
          openAll={openAll}
          onToggleOpenAll={onToggleOpenAll}
        />

        <InventoryTable
          rows={rows}
          layout={layout}
          openAll={openAll}
          pending={waiting}
          frameRef={frameRef}
          scrollerRef={scrollerRef}
        />

        <p className="mt-4 text-[0.75rem] leading-relaxed text-[#999]">
          Grouped by CTH catalogue number. Fragments of the same tablet family stay together. Data:
          TLHdig Beta 0.3 via Zenodo.
        </p>
      </div>
    </div>
  );
}

/** The two states that replace the page rather than appear inside it. */
function Notice({ tone, children }: { tone: "error" | "muted"; children: React.ReactNode }) {
  const wrapper =
    tone === "error"
      ? "flex min-h-screen items-center justify-center p-8 text-[#1a1a1a]"
      : "flex min-h-screen items-center justify-center p-8 text-[#666]";
  return (
    <div className={wrapper}>
      <p className={tone === "error" ? "text-sm text-red-700" : "text-sm"}>{children}</p>
    </div>
  );
}
