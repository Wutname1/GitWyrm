/**
 * Kill switch for the hands-on tour, off while it is still being worked on.
 *
 * The tour is feature-complete enough to build and typecheck but not to put in
 * front of users, and it is easier to hide the two entry points than to carry
 * the whole feature on a branch. Nothing else is stubbed out: the lessons, the
 * store, and the spotlight ids all still compile and can be exercised by
 * flipping this to `true`.
 *
 * To turn it back on, revert the commit that added this file -- it is the only
 * thing gating the feature, and the call sites that read it are the only edits
 * that came with it.
 */
export const TUTORIAL_ENABLED = false
