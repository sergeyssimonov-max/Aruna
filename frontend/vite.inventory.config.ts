/**
 * The build that produces `cli/src/generated/inventory_filter.js`.
 *
 * Separate from `vite.config.ts` because it produces something else entirely:
 * that one builds the desktop window, this one builds a script that is
 * compiled into the Rust binary and pasted into every inventory the program
 * writes. Run it with `pnpm build:inventory`.
 */
import { defineConfig } from 'vite'
import { CRATE_OUT, inventoryBuild } from './build/inventory.ts'

export default defineConfig(inventoryBuild(CRATE_OUT))
