/**
 * Entry point for the component preview page (`preview.html`).
 *
 * Separate from main.tsx because the real boot sequence calls Tauri commands
 * before it renders anything, and those do not exist in a plain browser -- the
 * app cannot start there at all. A separate entry keeps the production path
 * free of preview branches.
 *
 * Vite only builds the inputs listed in its config, and preview.html is not one
 * of them, so none of this reaches a release bundle.
 */
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { UpdateModalPreview } from './UpdateModalPreview'
import '../index.css'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <UpdateModalPreview />
  </StrictMode>,
)
