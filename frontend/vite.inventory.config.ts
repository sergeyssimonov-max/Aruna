/**
 * The build that produces `cli/src/generated/`: the inventory's script, its
 * three stylesheet sections and the markup of its eight components.
 *
 * Separate from `vite.config.ts` because it produces something else entirely:
 * that one builds the desktop window, this one builds what is compiled into
 * the Rust binary and pasted into every inventory the program writes. Run it
 * with `pnpm build:inventory`.
 */
import { defineConfig } from 'vite'
import { CRATE_OUT, inventoryBuild } from './build/inventory.ts'

export default defineConfig(inventoryBuild(CRATE_OUT))
