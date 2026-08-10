/**
 * Shared collision settings for every Radix overlay that portals to the body.
 *
 * The app's zoom is a CSS `zoom` on document.body (see useUiScale). That splits
 * the two numbers Radix compares when it decides whether a panel fits:
 * getBoundingClientRect on the trigger reports zoomed pixels, while
 * window.innerWidth/innerHeight stay in unzoomed CSS pixels. At 130% Radix
 * therefore thinks the viewport is ~30% wider than it is, concludes the panel
 * fits, and puts it past the edge of the window -- no amount of
 * collisionPadding helps, because the boundary itself is wrong.
 *
 * Handing it document.body fixes the mismatch: the body's own rect is measured
 * in the same zoomed space as the trigger, so both sides of the comparison
 * finally agree.
 *
 * This is read during render rather than stored in a module constant: the body
 * does not exist yet when this module is first evaluated under SSR-style
 * import order, and Radix re-reads the boundary each time a panel opens, which
 * is what lets one opened after a zoom change measure against the new size.
 */
export function overlayCollisionBoundary(): Element[] {
  return typeof document === 'undefined' ? [] : [document.body];
}

/** Keeps a panel this many pixels clear of the window edge. */
export const OVERLAY_COLLISION_PADDING = 8;
