/**
 * The entry Vite bundles into the exported inventory.
 *
 * One line, on purpose: everything the script does is in `filter.ts`, where a
 * test can drive it against a document it built itself. This file is the only
 * part that assumes it is running in the page.
 */
import { attachInventoryFilter } from './filter'

attachInventoryFilter(document)
