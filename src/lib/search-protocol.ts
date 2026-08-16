import type { SearchMatch } from "./inventory";

export type WorkerIn =
  | { type: "init"; arun: ArrayBuffer }
  | { type: "search"; id: number; q: string };

/**
 * Why search is not using the compact binary index.
 *
 * The page tells the reader when it has fallen back, so it has to know which
 * of these happened — the three are not interchangeable, and a note that
 * guesses is worse than one that says nothing.
 *
 * `"unsupported"` — the inventory does not fit the container: more distinct
 * editors or years than an id can address, a siglum past 255 bytes, a CTH past
 * `u16`. This is the one that depends on the corpus growing.
 *
 * `"unavailable"` — the index was built, but the module could not be fetched,
 * instantiated or made to accept it. Usually the environment: no WebAssembly,
 * a blocked or missing search module.
 *
 * `"trapped"` — a query made the loaded module trap, so the worker switched
 * engines mid-session and will not go back. Arrives after `ready`.
 */
export type FallbackReason = "unsupported" | "unavailable" | "trapped";

export type WorkerOut =
  | {
      type: "ready";
      manuscripts: number;
      groups: number;
      engine: "wasm" | "js";
      /** Absent when `engine` is `"wasm"`. */
      reason?: FallbackReason;
    }
  /** The engine changed after `ready` — currently only wasm → js on a trap. */
  | { type: "engine"; engine: "js"; reason: FallbackReason }
  | { type: "result"; id: number; matches: SearchMatch[] }
  | { type: "error"; message: string };
