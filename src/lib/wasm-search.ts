/**
 * The Rust search module, and the pointer arithmetic needed to talk to it.
 *
 * This is the only place in the app that deals in offsets into another
 * language's heap, so it is also the only place that has to be careful about
 * two things: every allocation is handed back, and no view into WASM memory
 * outlives a call that might have grown it.
 *
 * Every path returns null rather than throwing. The caller has a JavaScript
 * index built and ready, so a module that will not load is a downgrade, not a
 * failure — see `search.worker.ts`, which decides which engine answers.
 */
import type { SearchMatch } from "./inventory";
// Imported rather than written out as `/wasm/search.wasm`, so the build emits
// the module under a name carrying a hash of its contents. The fetch below
// asks for it with `force-cache`: on a stable name that meant a visitor kept
// whichever module they first loaded, however often the search code changed
// underneath them.
import WASM_URL from "@/wasm/search.wasm?url";

type Exports = {
  memory: WebAssembly.Memory;
  alloc(n: number): number;
  dealloc(ptr: number, n: number): void;
  init(ptr: number, len: number): number;
  reset(): void;
  search(qPtr: number, qLen: number, outPtr: number, outCap: number): number;
};

/**
 * Layout of the buffer `search` writes back: a `u32` count, then that many
 * entries of three `u32`s — group index, kind, item index within the group.
 *
 * Mirrors `RESULT_STRIDE` and the entry order in `wasm/search/src/format.rs`;
 * `tlh2-agreement.test.ts` checks this side against that one.
 */
const RESULT_COUNT_BYTES = 4;
const RESULT_STRIDE = 12;
/** Kinds an entry can be: the group's own label matched, or one manuscript did. */
const WHOLE_GROUP = 0;

/**
 * Room for the answer to any query.
 *
 * Worst case is every manuscript matching: 24k entries × 12 B + 4 ≈ 288 KB.
 * A megabyte is allocated once, at load, and reused by every search — the
 * module truncates to what fits rather than failing, so this is a ceiling on
 * results, not a buffer that can overflow.
 */
const OUT_CAP = 1 << 20;

/** Fetch and instantiate the module, checking it exports what we call. */
async function instantiate(): Promise<Exports | null> {
  const res = await fetch(WASM_URL, { credentials: "same-origin", cache: "force-cache" });
  if (!res.ok) return null;

  const { instance } = await WebAssembly.instantiate(await res.arrayBuffer(), {});
  const exports = instance.exports as unknown as Exports;
  const complete =
    typeof exports.alloc === "function" &&
    typeof exports.dealloc === "function" &&
    typeof exports.init === "function" &&
    typeof exports.search === "function" &&
    typeof exports.reset === "function" &&
    exports.memory instanceof WebAssembly.Memory;
  return complete ? exports : null;
}

/**
 * Copy `bytes` into the module's heap, run `borrow` with the pointer, free it.
 *
 * The view is taken after `alloc`, never before: allocating can grow the
 * module's memory, which detaches every existing view of it.
 */
function withBytes<T>(exports: Exports, bytes: Uint8Array, borrow: (ptr: number) => T): T | null {
  const ptr = exports.alloc(bytes.length);
  if (!ptr) return null;
  try {
    new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes);
    return borrow(ptr);
  } finally {
    exports.dealloc(ptr, bytes.length);
  }
}

/**
 * Read the result buffer into matches, folding a run of item hits in one group
 * into a single entry — which is the shape the page renders from.
 */
function decodeMatches(view: DataView, count: number): SearchMatch[] {
  const at = (index: number, word: number) =>
    view.getUint32(RESULT_COUNT_BYTES + index * RESULT_STRIDE + word * 4, true);

  const matches: SearchMatch[] = [];
  let i = 0;
  while (i < count) {
    const group = at(i, 0);
    if (at(i, 1) === WHOLE_GROUP) {
      matches.push({ group, items: null });
      i++;
      continue;
    }
    // The module writes a group's items consecutively, so a run of them is one
    // match rather than one per manuscript.
    const items: number[] = [];
    while (i < count && at(i, 0) === group && at(i, 1) !== WHOLE_GROUP) {
      items.push(at(i, 2));
      i++;
    }
    matches.push({ group, items });
  }
  return matches;
}

/** Thin glue around the Rust cdylib search module. */
export class WasmSearch {
  private constructor(
    private readonly exports: Exports,
    /** The result buffer, held for the module's lifetime. */
    private readonly outPtr: number,
  ) {}

  /**
   * Load the module and hand it `index`, or return null if any step declines.
   *
   * The index is copied in and freed straight away: `init` keeps its own copy,
   * so leaving ours would be a second 600 KB in the module's heap for good.
   */
  static async create(index: ArrayBuffer): Promise<WasmSearch | null> {
    try {
      const exports = await instantiate();
      if (!exports) return null;

      const loaded = withBytes(exports, new Uint8Array(index), (ptr) =>
        exports.init(ptr, index.byteLength),
      );
      if (!loaded) return null;

      const outPtr = exports.alloc(OUT_CAP);
      if (!outPtr) return null;
      return new WasmSearch(exports, outPtr);
    } catch {
      return null;
    }
  }

  search(query: string): SearchMatch[] {
    const { exports, outPtr } = this;
    const bytes = new TextEncoder().encode(query);

    const count =
      bytes.length === 0
        ? exports.search(0, 0, outPtr, OUT_CAP)
        : withBytes(exports, bytes, (ptr) => exports.search(ptr, bytes.length, outPtr, OUT_CAP));
    if (count === null) return [];

    // Read the buffer only now: `search` itself cannot grow memory, but the
    // allocation for the query above may have, and a view taken before that
    // would be detached.
    const view = new DataView(exports.memory.buffer, outPtr, OUT_CAP);
    // The module returns the count and also writes it into the buffer; trust
    // the smaller of the two rather than either alone.
    return decodeMatches(view, Math.min(count, view.getUint32(0, true)));
  }

  dispose() {
    try {
      this.exports.reset();
      this.exports.dealloc(this.outPtr, OUT_CAP);
    } catch {
      // Disposing a module that has already gone is not worth reporting.
    }
  }
}
