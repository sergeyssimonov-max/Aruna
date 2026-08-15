import { IconSearch } from "./icons";

/** The query field and the expand/collapse switch. */
export function SearchBar({
  query,
  onQuery,
  openAll,
  onToggleOpenAll,
}: {
  query: string;
  onQuery: (value: string) => void;
  openAll: boolean;
  onToggleOpenAll: () => void;
}) {
  return (
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
        onClick={onToggleOpenAll}
        className="shrink-0 rounded-md border border-[#e8e8e8] bg-white px-3 py-2.5 text-sm text-[#444] hover:bg-[#fafafa]"
      >
        {openAll ? "Collapse fragments" : "Expand fragments"}
      </button>
    </div>
  );
}
