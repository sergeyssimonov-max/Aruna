import type { SearchMatch } from "./inventory";

export type WorkerIn =
  | { type: "init"; arun: ArrayBuffer }
  | { type: "search"; id: number; q: string };

export type WorkerOut =
  | { type: "ready"; manuscripts: number; groups: number; engine: "wasm" | "js" }
  | { type: "result"; id: number; matches: SearchMatch[] }
  | { type: "error"; message: string };
