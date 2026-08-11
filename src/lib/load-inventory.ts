import { isArun, parseInventory } from "./arun";
import type { Inventory } from "./inventory";

const BIN_URL = "/data/inventory.bin";
const GZIP_URL = "/data/inventory.bin.gz";

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
 * Load ARUN (.gz preferred). One network hop.
 * Returns display model + binary for the search worker.
 */
export async function loadInventory(signal?: AbortSignal): Promise<LoadedInventory> {
  let arun: ArrayBuffer;
  try {
    arun = await fetchBuf(GZIP_URL, signal);
  } catch {
    arun = await fetchBuf(BIN_URL, signal);
  }
  return { inventory: parseInventory(arun), arun };
}
