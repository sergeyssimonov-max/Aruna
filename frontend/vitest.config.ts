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
      exclude: ['src/**/*.test.ts', 'src/vite-env.d.ts'],
    },
  },
})
