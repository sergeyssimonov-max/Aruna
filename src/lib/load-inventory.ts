import { isArun, parseInventory } from "./arun";
import type { Inventory } from "./inventory";

// Imported rather than written out as `/data/inventory.bin.gz`, so the build
// emits each file under a name carrying a hash of its contents.
//
// The name is what makes `force-cache` below safe. A stable path plus a cache
// that is told never to revalidate is how a visitor keeps yesterday's catalog
// after a deploy — the file they hold answers to the same URL as the new one,
// and nothing asks the server whether it changed. A content hash makes new
// data a new URL, so the old entry is simply never asked for again, and the
// one they do hold can be kept forever.
import BIN_URL from "@/data/inventory.bin?url";
import GZIP_URL from "@/data/inventory.bin.gz?url";

const fetchOpts: RequestInit = {
  credentials: "same-origin",
  cache: "force-cache",
};

async function gunzipIfNeeded(buf: ArrayBuffer): Promise<ArrayBuffer> {
  if (isArun(buf)) return buf;
  const u8 = new Uint8Array(buf);
  if (u8.length >= 2 && u8[0] === 0x1f && u8[1] === 0x8b) {
    if (typeof DecompressionStream !== "function") {
      throw new Error("gzip inventory requires DecompressionStream");
    }
    const ds = new DecompressionStream("gzip");
    const stream = new Response(buf).body!.pipeThrough(ds);
    const out = await new Response(stream).arrayBuffer();
    if (!isArun(out)) throw new Error("inventory: gzip is not ARUN");
    return out;
  }
  throw new Error("inventory: not ARUN");
}

async function fetchBuf(url: string, signal?: AbortSignal): Promise<ArrayBuffer> {
  const res = await fetch(url, { ...fetchOpts, signal });
  if (!res.ok) throw new Error(`Failed to load inventory (${res.status})`);
  return gunzipIfNeeded(await res.arrayBuffer());
}

export type LoadedInventory = {
  inventory: Inventory;
  /** Raw ARUN bytes — hand to the search worker (copy; main keeps its parse). */
  arun: ArrayBuffer;
};

/**
 * Load ARUN (.gz preferred, uncompressed as a fallback). One network hop.
 * Returns display model + binary for the search worker.
 */
export async function loadInventory(signal?: AbortSignal): Promise<LoadedInventory> {
  let arun: ArrayBuffer;
  try {
    arun = await fetchBuf(GZIP_URL, signal);
  } catch (gzipError) {
    // A cancelled load is not a missing file. The catch used to swallow
    // everything, so navigating away — which aborts the request — read as "the
    // gzip is unavailable, try the plain one". A browser short-circuits that
    // second request because the signal is already aborted, so nothing extra
    // went over the wire; what it did do was turn a deliberate cancellation
    // into a fetch that can only fail, and report it as a load error.
    if (isAbort(gzipError, signal)) throw gzipError;

    try {
      arun = await fetchBuf(BIN_URL, signal);
    } catch (binError) {
      if (isAbort(binError, signal)) throw binError;
      // Both failed. The gzip is the file that is supposed to be there, so its
      // failure is the one worth reporting — reporting only the fallback's
      // hides a corrupt .gz behind a message about a file the deployment may
      // not even publish.
      throw new Error(
        `Failed to load inventory: ${message(gzipError)} (fallback: ${message(binError)})`,
        { cause: gzipError },
      );
    }
  }
  return { inventory: parseInventory(arun), arun };
}

/** Whether a rejection means "the caller cancelled", not "the load failed". */
function isAbort(error: unknown, signal?: AbortSignal): boolean {
  if (signal?.aborted) return true;
  return error instanceof DOMException && error.name === "AbortError";
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
