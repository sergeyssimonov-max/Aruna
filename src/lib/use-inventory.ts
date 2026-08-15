import { useEffect, useState } from "react";
import type { Inventory } from "./inventory";
import { loadInventory } from "./load-inventory";

export type InventoryState = {
  /** The parsed inventory, or null while it is still loading. */
  data: Inventory | null;
  /** The raw ARUN bytes, handed to the search worker. */
  arun: ArrayBuffer | null;
  error: string | null;
};

/**
 * Load the inventory once, and drop the result if the page moved on.
 *
 * The abort matters twice over: it stops the fetch, and it is what tells this
 * hook not to call `setState` for a load nobody is waiting for any more.
 * `loadInventory` rethrows an abort rather than reporting it as a failure —
 * see the note there — so a cancelled load leaves the state untouched instead
 * of putting "Load failed" on a page that is going away.
 */
export function useInventory(): InventoryState {
  const [data, setData] = useState<Inventory | null>(null);
  const [arun, setArun] = useState<ArrayBuffer | null>(null);
  const [error, setError] = useState<string | null>(null);

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
        setError(e instanceof Error ? e.message : "Load failed");
      }
    })();
    return () => ac.abort();
  }, []);

  return { data, arun, error };
}
