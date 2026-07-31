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
 * Scope is `src/lib` because that is where the testable logic lives -- diffing,
 * branch trees, graph columns, version compare. Components are not covered:
 * that would need a DOM environment and a testing library, which is a larger
 * decision than this. The app is still verified by running it.
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
