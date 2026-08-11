import { createFileRoute } from "@tanstack/react-router";
import {
  memo,
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  applyMatches,
  type Group,
  type Inventory,
} from "@/lib/inventory";
import { loadInventory } from "@/lib/load-inventory";
import { useSearchWorker } from "@/lib/use-search-worker";
import {
  OVERSCAN_PX,
  buildLayout,
  countItems,
  visibleRows,
} from "@/lib/virtual-list";

export const Route = createFileRoute("/")({
  component: InventoryPage,
  head: () => ({
    meta: [{ title: "Thesaurus Linguarum Hethaeorum Digitalis" }],
  }),
});

const IconSearch = memo(function IconSearch({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" aria-hidden>
      <circle cx="11" cy="11" r="7" />
      <path d="M20 20l-3.5-3.5" strokeLinecap="round" />
    </svg>
  );
});

const IconChevron = memo(function IconChevron({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" aria-hidden>
      <path d="M9 6l6 6-6 6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
});

const IconClayTablet = memo(function IconClayTablet({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden>
      <rect width="32" height="32" rx="8" fill="#F3EDE3" />
      <rect
        x="7.5"
        y="4"
        width="17"
        height="24"
        rx="2"
        stroke="#B89A6E"
        strokeWidth="1.1"
        strokeLinejoin="round"
      />
      <g
        fill="none"
        stroke="#A08058"
        strokeWidth="1.1"
        strokeLinejoin="round"
        strokeLinecap="round"
      >
        <path d="M11.2 9h3.2l-1.6 2.3z" />
        <path d="M15.4 9h3.2l-1.6 2.3z" />
        <path d="M19.6 9h3.2l-1.6 2.3z" />
        <path d="M11.2 13.4h3.2l-1.6 2.3z" />
        <path d="M15.4 13.4h3.2l-1.6 2.3z" />
        <path d="M19.6 13.4h3.2l-1.6 2.3z" />
        <path d="M11.2 17.8h3.2l-1.6 2.3z" />
        <path d="M15.4 17.8h3.2l-1.6 2.3z" />
        <path d="M19.6 17.8h3.2l-1.6 2.3z" />
        <path d="M13.3 22.2h3.2l-1.6 2.3z" />
        <path d="M17.5 22.2h3.2l-1.6 2.3z" />
      </g>
    </svg>
  );
});

const GroupRow = memo(function GroupRow({
  c,
  n,
  y,
}: {
  c: string;
  n: number;
  y: number;
}) {
  return (
    <div className="vl-row vl-group" style={{ transform: `translate3d(0,${y}px,0)` }}>
      <IconChevron className="h-3.5 w-3.5 shrink-0 text-[#bbb]" />
      <span className="text-[0.95rem] font-semibold tracking-tight">{c}</span>
      <span className="text-[0.75rem] tabular-nums text-[#999]">· {n}</span>
    </div>
  );
});

const ItemRow = memo(function ItemRow({
  n,
  s,
  l,
  corpus,
  a,
  yv,
  y,
}: {
  n: number;
  s: string;
  l: string;
  corpus: string;
  a: string;
  yv: string;
  y: number;
}) {
  return (
    <div className="vl-row vl-item" style={{ transform: `translate3d(0,${y}px,0)` }}>
      <div className="vl-c-num tabular-nums text-[#999]">{n}</div>
      <div className="vl-c-sig min-w-0 truncate font-medium" title={s}>{s}</div>
      <div className="vl-c-lang tabular-nums text-[#444]">{l}</div>
      <div className="vl-c-corp truncate text-[#444]" title={corpus}>{corpus}</div>
      <div className="vl-c-ed min-w-0 truncate text-[#444]" title={a}>{a}</div>
      <div className="vl-c-year tabular-nums text-[#444]">{yv}</div>
    </div>
  );
});

function InventoryPage() {
  const [data, setData] = useState<Inventory | null>(null);
  const [arun, setArun] = useState<ArrayBuffer | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const [openAll, setOpenAll] = useState(true);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const frameRef = useRef<HTMLDivElement>(null);
  const scrollTopRef = useRef(0);
  const rangeRef = useRef({ y0: 0, y1: 720 });
  const [range, setRange] = useState({ y0: 0, y1: 720 });
  const rafRef = useRef(0);

  const { status: workerStatus, error: workerError, matches, search } =
    useSearchWorker(arun);

  useEffect(() => {
    const ac = new AbortController();
    (async () => {
      try {
        const loaded = await loadInventory(ac.signal);
        if (ac.signal.aborted) return;
        setData(loaded.inventory);
        setArun(loaded.arun);
      } catch (e) {
        if (ac.signal.aborted) return;
        setLoadError(e instanceof Error ? e.message : "Load failed");
      }
    })();
    return () => ac.abort();
  }, []);

  useEffect(() => {
    search(deferredQuery);
  }, [deferredQuery, search, workerStatus]);

  const filtered: Group[] = useMemo(() => {
    if (!data) return [];
    if (!deferredQuery.trim()) return data.groups;
    if (!matches) return [];
    return applyMatches(data, matches);
  }, [data, deferredQuery, matches]);

  const layout = useMemo(
    () => buildLayout(filtered, openAll),
    [filtered, openAll],
  );

  // Scroll window — rAF coalesced, hysteresis avoids thrash near edges.
  useEffect(() => {
    const el = scrollerRef.current;
    if (!el) return;

    const publish = (st: number, vh: number) => {
      scrollTopRef.current = st;
      const prev = rangeRef.current;
      const margin = OVERSCAN_PX * 0.45;
      if (st >= prev.y0 + margin && st + vh <= prev.y1 - margin) return;
      const y0 = st > OVERSCAN_PX ? st - OVERSCAN_PX : 0;
      const y1 = st + vh + OVERSCAN_PX;
      const next = { y0, y1 };
      rangeRef.current = next;
      setRange(next);
    };

    const onScroll = () => {
      if (rafRef.current) return;
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = 0;
        publish(el.scrollTop, el.clientHeight);
      });
    };

    const syncSb = () => {
      const sb = el.offsetWidth - el.clientWidth;
      if (frameRef.current) {
        frameRef.current.style.setProperty("--vl-sb", `${Math.max(0, sb)}px`);
      }
    };
    const ro = new ResizeObserver(() => {
      syncSb();
      publish(el.scrollTop, el.clientHeight);
    });
    ro.observe(el);
    syncSb();
    publish(el.scrollTop, el.clientHeight || 600);

    el.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      el.removeEventListener("scroll", onScroll);
      ro.disconnect();
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, [data]);

  // Reset window on filter / expand change.
  useEffect(() => {
    scrollTopRef.current = 0;
    const vh = scrollerRef.current?.clientHeight ?? 600;
    const next = { y0: 0, y1: vh + OVERSCAN_PX };
    rangeRef.current = next;
    setRange(next);
    scrollerRef.current?.scrollTo({ top: 0 });
  }, [filtered, openAll]);

  const rows = useMemo(
    () => visibleRows(filtered, layout, openAll, range.y0, range.y1),
    [filtered, layout, openAll, range.y0, range.y1],
  );

  const visibleCount = useMemo(() => {
    if (openAll) return layout.itemBase[layout.groupCount]!;
    return countItems(filtered);
  }, [filtered, layout, openAll]);

  const onQuery = useCallback((v: string) => setQuery(v), []);

  const searching = deferredQuery.trim() !== "";
  const searchPending =
    searching &&
    (workerStatus !== "ready" ||
      (matches === null && deferredQuery.trim() !== ""));

  const error = loadError || workerError;
  if (error) {
    return (
      <div className="flex min-h-screen items-center justify-center p-8 text-[#1a1a1a]">
        <p className="text-sm text-red-700">{error}</p>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="flex min-h-screen items-center justify-center p-8 text-[#666]">
        <p className="text-sm">Loading inventory…</p>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-[#fafafa] text-[#1a1a1a]">
      <div className="mx-auto max-w-4xl px-4 pt-[calc(var(--grok-banner-h,0px)+1.5rem)] pb-10 sm:px-6">
        <header className="mb-6 sm:mb-8">
          <div className="mb-3 flex items-start gap-3">
            <div className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-lg">
              <IconClayTablet className="h-10 w-10" />
            </div>
            <div className="min-w-0">
              <h1 className="text-[1.25rem] font-semibold leading-snug tracking-tight sm:text-[1.35rem]">
                Thesaurus Linguarum Hethaeorum Digitalis
              </h1>
              <p className="mt-1 text-[0.8125rem] leading-relaxed text-[#666]">
                {data.source}
              </p>
            </div>
          </div>
          <div className="flex flex-wrap gap-x-4 gap-y-1 text-[0.8125rem] text-[#666]">
            <span>
              Manuscripts:{" "}
              <strong className="font-medium tabular-nums text-[#1a1a1a]">
                {data.manuscripts.toLocaleString()}
              </strong>
            </span>
            <span>
              CTH groups:{" "}
              <strong className="font-medium tabular-nums text-[#1a1a1a]">
                {data.groups.length.toLocaleString()}
              </strong>
            </span>
            {searching && (
              <span>
                Match:{" "}
                <strong className="font-medium tabular-nums text-[#1a1a1a]">
                  {visibleCount.toLocaleString()}
                </strong>
                {searchPending ? <span className="ml-1 text-[#aaa]">…</span> : null}
              </span>
            )}
          </div>
        </header>

        <section className="col-legend mb-4" aria-label="Column legend">
          <p className="col-legend-title">Columns</p>
          <ul className="col-legend-list">
            <li>
              <span className="col-legend-key">№</span>
              <span className="col-legend-def">row number</span>
            </li>
            <li>
              <span className="col-legend-key">Siglum</span>
              <span className="col-legend-def">publication id (e.g. KBo 3.22)</span>
            </li>
            <li>
              <span className="col-legend-key">Lang</span>
              <span className="col-legend-def">dominant language (Hit, Hur, Akk…)</span>
            </li>
            <li>
              <span className="col-legend-key">Corpus</span>
              <span className="col-legend-def">edition series (HFR, TLH, HAnn…)</span>
            </li>
            <li>
              <span className="col-legend-key">Editor</span>
              <span className="col-legend-def">transliteration / edition author</span>
            </li>
            <li>
              <span className="col-legend-key">Year</span>
              <span className="col-legend-def">edition year</span>
            </li>
          </ul>
        </section>

        <div className="mb-4 flex flex-col gap-2 sm:flex-row sm:items-center">
          <label className="relative flex-1">
            <IconSearch className="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-[#999]" />
            <input
              type="search"
              value={query}
              onChange={(e) => onQuery(e.target.value)}
              placeholder="Search CTH, siglum, lang, corpus, editor, year…"
              autoComplete="off"
              spellCheck={false}
              className="w-full rounded-md border border-[#e8e8e8] bg-white py-2.5 pr-3 pl-10 text-sm outline-none placeholder:text-[#aaa] focus:border-[#ccc] focus:ring-2 focus:ring-[#eee]"
            />
          </label>
          <button
            type="button"
            onClick={() => setOpenAll((v) => !v)}
            className="shrink-0 rounded-md border border-[#e8e8e8] bg-white px-3 py-2.5 text-sm text-[#444] hover:bg-[#fafafa]"
          >
            {openAll ? "Collapse fragments" : "Expand fragments"}
          </button>
        </div>

        <div
          ref={frameRef}
          className="vl-frame overflow-hidden rounded-md border border-[#e8e8e8] bg-white"
          style={{ height: "min(70vh, 720px)" }}
        >
          <div className="vl-head-bar">
            <div className="vl-head" role="row">
              <div className="vl-c-num">№</div>
              <div className="vl-c-sig">Siglum</div>
              <div className="vl-c-lang">Lang</div>
              <div className="vl-c-corp">Corpus</div>
              <div className="vl-c-ed">Editor</div>
              <div className="vl-c-year">Year</div>
            </div>
          </div>
          <div ref={scrollerRef} className="vl-scroll">
            <div
              className="vl-spacer"
              style={{ height: layout.totalH }}
              aria-rowcount={
                openAll ? layout.itemBase[layout.groupCount]! : layout.groupCount
              }
            >
              {rows.map((row) =>
                row.t === 0 ? (
                  <GroupRow key={row.key} c={row.c} n={row.n} y={row.y} />
                ) : (
                  <ItemRow
                    key={row.key}
                    n={row.n}
                    s={row.s}
                    l={row.l}
                    corpus={row.corpus}
                    a={row.a}
                    yv={row.yv}
                    y={row.y}
                  />
                ),
              )}
            </div>
          </div>
        </div>

        <p className="mt-4 text-[0.75rem] leading-relaxed text-[#999]">
          Grouped by CTH catalogue number. Fragments of the same tablet family stay
          together. Data: TLHdig Beta 0.3 via Zenodo.
        </p>
      </div>
    </div>
  );
}
