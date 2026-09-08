import { defineConfig } from 'vitest/config'
import path from 'node:path'

/**
 * Unit tests for pure logic in `src/lib`.
 *
 * Separate from `vite.config.ts` on purpose: the app config carries the React
 * and Tailwind plugins and the Sentry source-map upload, none of which a
 * headless logic test needs, and one of which talks to the network when a token
 * is present.
 *
 * `include` is every `*.test.ts` under `src`, not only `src/lib`. Most of the
 * testable logic does live there -- diffing, branch trees, graph columns,
 * version compare -- but stores and a few components export pure functions
 * worth pinning beside themselves, and ten test files already sit outside
 * `src/lib` on that basis. This comment used to say the scope was `src/lib`,
 * which would have told the next person their colocated test was never run.
 *
 * What is still not covered is anything needing a DOM: rendering, events,
 * hooks. That would need an environment and a testing library, which is a
 * larger decision than this. Note the `.ts` extension in the pattern -- a
 * `.test.tsx` is not picked up, which is the line between the two.
 */
export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
})
