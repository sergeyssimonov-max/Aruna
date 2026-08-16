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

// Worst case: every item is a hit → 24k entries * 12 + 4 ≈ 288 KB; allocate 1 MB.
const OUT_CAP = 1 << 20;

/**
 * Thin JS glue around the Rust cdylib search module.
 * Falls back to null if WASM fails to load (caller uses JS search).
 */
export class WasmSearch {
  private exp: Exports;
  private outPtr: number;
  private te = new TextEncoder();

  private constructor(exp: Exports, outPtr: number) {
    this.exp = exp;
    this.outPtr = outPtr;
  }

  static async create(index: ArrayBuffer): Promise<WasmSearch | null> {
    try {
      const res = await fetch(WASM_URL, { credentials: "same-origin", cache: "force-cache" });
      if (!res.ok) return null;
      const raw = await res.arrayBuffer();
      const { instance } = await WebAssembly.instantiate(raw, {});
      const exp = instance.exports as unknown as Exports;
      if (
        typeof exp.alloc !== "function" ||
        typeof exp.init !== "function" ||
        typeof exp.search !== "function" ||
        !exp.memory
      ) {
        return null;
      }

      // Copy index into WASM memory.
      const idxLen = index.byteLength;
      const idxPtr = exp.alloc(idxLen);
      if (!idxPtr) return null;
      new Uint8Array(exp.memory.buffer, idxPtr, idxLen).set(new Uint8Array(index));
      const ok = exp.init(idxPtr, idxLen);
      exp.dealloc(idxPtr, idxLen);
      if (!ok) return null;

      const outPtr = exp.alloc(OUT_CAP);
      if (!outPtr) return null;

      return new WasmSearch(exp, outPtr);
    } catch {
      return null;
    }
  }

  search(q: string): SearchMatch[] {
    const { exp, outPtr, te } = this;
    const qBytes = te.encode(q);
    let qPtr = 0;
    if (qBytes.length) {
      qPtr = exp.alloc(qBytes.length);
      if (!qPtr) return [];
      // memory may have grown — re-read buffer after alloc
      new Uint8Array(exp.memory.buffer, qPtr, qBytes.length).set(qBytes);
    }

    const count = exp.search(qPtr, qBytes.length, outPtr, OUT_CAP);
    if (qPtr) exp.dealloc(qPtr, qBytes.length);

    const view = new DataView(exp.memory.buffer, outPtr, OUT_CAP);
    // Prefer header count; clamp to returned value.
    const headerCount = view.getUint32(0, true);
    const n = Math.min(count, headerCount);

    // Coalesce consecutive item hits for the same group into one SearchMatch.
    const out: SearchMatch[] = [];
    let i = 0;
    while (i < n) {
      const base = 4 + i * 12;
      const group = view.getUint32(base, true);
      const kind = view.getUint32(base + 4, true);
      const extra = view.getUint32(base + 8, true);
      if (kind === 0) {
        out.push({ group, items: null });
        i++;
        continue;
      }
      // Gather run of items for this group.
      const items: number[] = [extra];
      i++;
      while (i < n) {
        const b2 = 4 + i * 12;
        const group2 = view.getUint32(b2, true);
        const kind2 = view.getUint32(b2 + 4, true);
        if (group2 !== group || kind2 !== 1) break;
        items.push(view.getUint32(b2 + 8, true));
        i++;
      }
      out.push({ group, items });
    }
    return out;
  }

  dispose() {
    try {
      this.exp.reset();
      this.exp.dealloc(this.outPtr, OUT_CAP);
    } catch {
      /* ignore */
    }
  }
}
