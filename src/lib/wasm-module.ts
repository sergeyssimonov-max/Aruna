/**
 * Fetching the compiled search module, and nothing else.
 *
 * Split from `wasm-search.ts` so that file can be exercised outside a browser:
 * everything here — the bundler's URL for the module, `fetch`, the caching
 * rules — needs a page, while the pointer arithmetic next door needs only bytes.
 */

// Imported rather than written out as `/wasm/search.wasm`, so the build emits
// the module under a name carrying a hash of its contents. The fetch below
// asks for it with `force-cache`: on a stable name that meant a visitor kept
// whichever module they first loaded, however often the search code changed
// underneath them.
import WASM_URL from "@/wasm/search.wasm?url";

/**
 * The module's bytes, or null if they cannot be had.
 *
 * Null rather than a throw, like every other step of loading the fast engine:
 * the JavaScript index is built and ready, so a module that will not arrive is
 * a downgrade the page reports, not a failure it has to survive.
 */
export async function fetchWasmModule(): Promise<ArrayBuffer | null> {
  try {
    const res = await fetch(WASM_URL, { credentials: "same-origin", cache: "force-cache" });
    if (!res.ok) return null;
    return await res.arrayBuffer();
  } catch {
    return null;
  }
}
