import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [svelte()],
  // Top level, not inside the `component` project. Vitest transforms test files
  // through the SSR pipeline, so without this Svelte resolves to its server
  // build and `mount()` throws `lifecycle_function_unavailable` — which is what
  // `src/lib/Counter.test.ts` did with the condition set on the project alone.
  resolve: {
    conditions: ['browser'],
  },
  // `cli/src/generated/` is above this root, and `filter.test.ts` reads the
  // artifacts out of it: the document and the fragments the crate compiles in
  // are the shape the renderer writes, and a fixture that described that shape
  // in its own words was a second description of one table.
  server: { fs: { allow: ['..'] } },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'node',
          environment: 'node',
          include: ['tests/**/*.test.ts'],
        },
      },
      {
        extends: true,
        resolve: {
          conditions: ['browser'],
        },
        test: {
          name: 'component',
          environment: 'jsdom',
          setupFiles: ['./tests/setup.ts'],
          include: ['src/**/*.test.ts'],
        },
      },
    ],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html'],
      include: ['src/**/*.{ts,svelte}'],
      // Only the tests. `src/vite-env.d.ts` was named here too and no such file
      // exists — the client types come from `"types": ["vite/client"]` in
      // tsconfig.app.json instead. An exclude that matches nothing is a claim
      // about the tree that stopped being true without anything failing.
      exclude: ['src/**/*.test.ts'],
    },
  },
})
