import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { sentryVitePlugin } from '@sentry/vite-plugin'
import path from 'node:path'
import pkg from './package.json'

// Upload JS source maps to Sentry so production stack traces de-minify. Only
// runs when SENTRY_AUTH_TOKEN is present (CI sets it from a secret), so a local
// `npm run build` without the token still succeeds -- it just skips the upload.
const sentryToken = process.env.SENTRY_AUTH_TOKEN

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  // Source maps must be emitted for the plugin to upload them. The plugin
  // deletes them from the bundle after upload so they never ship to users.
  build: sentryToken ? { sourcemap: true } : {},
  plugins: [
    react(),
    tailwindcss(),
    ...(sentryToken
      ? [
          sentryVitePlugin({
            org: 'gitwyrm',
            project: 'gitwyrm-frontend',
            authToken: sentryToken,
            release: { name: pkg.version },
            sourcemaps: { filesToDeleteAfterUpload: ['**/*.map'] },
          }),
        ]
      : []),
  ],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
})
