import { memo } from "react";
import { COLUMNS } from "@/lib/columns";
import type { Layout, VisRow } from "@/lib/virtual-list";
import { IconChevron } from "./icons";

/**
 * The scrolling table: a heading row that does not move, and a spacer as tall
 * as the whole inventory holding only the rows currently in view.
 *
 * Which rows those are is decided outside — `rows` is already the answer. This
 * component owns no scrolling logic; it only lends its two elements to
 * `useScrollWindow`, which measures the scroller and writes the scrollbar
 * width to the frame.
 */
export function InventoryTable({
  rows,
  layout,
  openAll,
  pending,
  frameRef,
  scrollerRef,
}: {
  rows: VisRow[];
  layout: Layout;
  openAll: boolean;
  /** A query is in flight: these rows answer the previous one. */
  pending: boolean;
  frameRef: React.RefObject<HTMLDivElement | null>;
  scrollerRef: React.RefObject<HTMLDivElement | null>;
}) {
  return (
    <div
      ref={frameRef}
      className="vl-frame overflow-hidden rounded-md border border-[#e8e8e8] bg-white"
      style={{ height: "min(70vh, 720px)" }}
    >
      <div className="vl-head-bar">
        <div className="vl-head" role="row">
          {COLUMNS.map((column) => (
            <div key={column.className} className={column.className}>
              {column.head}
            </div>
          ))}
        </div>
      </div>
      {/* Dimmed while a query is in flight, so rows that answer the previous
          one are visibly not the answer yet. `aria-busy` says the same thing to
          a screen reader, which cannot see the dimming. */}
      <div
        ref={scrollerRef}
        className={`vl-scroll transition-opacity duration-150 ${pending ? "opacity-50" : ""}`}
        aria-busy={pending}
      >
        <div
          className="vl-spacer"
          style={{ height: layout.totalH }}
          aria-rowcount={openAll ? layout.itemBase[layout.groupCount]! : layout.groupCount}
        >
          {rows.map((row) =>
            row.kind === "group" ? (
              <GroupRow key={row.key} cth={row.cth} count={row.count} top={row.top} />
            ) : (
              <ItemRow
                key={row.key}
                number={row.number}
                siglum={row.siglum}
                lang={row.lang}
                corpus={row.corpus}
                editor={row.editor}
                year={row.year}
                top={row.top}
              />
            ),
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Rows are absolutely positioned by `top` and memoised: scrolling changes
 * which rows exist, not the contents of the ones that stay.
 */
const GroupRow = memo(function GroupRow({
  cth,
  count,
  top,
}: {
  cth: string;
  count: number;
  top: number;
}) {
  return (
    <div className="vl-row vl-group" style={{ transform: `translate3d(0,${top}px,0)` }}>
      <IconChevron className="h-3.5 w-3.5 shrink-0 text-[#bbb]" />
      <span className="text-[0.95rem] font-semibold tracking-tight">{cth}</span>
      <span className="text-[0.75rem] tabular-nums text-[#999]">· {count}</span>
    </div>
  );
});

const ItemRow = memo(function ItemRow({
  number,
  siglum,
  lang,
  corpus,
  editor,
  year,
  top,
}: {
  number: number;
  siglum: string;
  lang: string;
  corpus: string;
  editor: string;
  year: string;
  top: number;
}) {
  return (
    <div className="vl-row vl-item" style={{ transform: `translate3d(0,${top}px,0)` }}>
      <div className="vl-c-num tabular-nums text-[#999]">{number}</div>
      <div className="vl-c-sig min-w-0 truncate font-medium" title={siglum}>
        {siglum}
      </div>
      <div className="vl-c-lang tabular-nums text-[#444]">{lang}</div>
      <div className="vl-c-corp truncate text-[#444]" title={corpus}>
        {corpus}
      </div>
      <div className="vl-c-ed min-w-0 truncate text-[#444]" title={editor}>
        {editor}
      </div>
      <div className="vl-c-year tabular-nums text-[#444]">{year}</div>
    </div>
  );
});
