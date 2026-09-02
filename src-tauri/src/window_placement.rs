//! Keeping windows where a person can reach them.
//!
//! The window-state plugin restores each window's last position on open. That
//! is right until the screens change underneath it: a laptop undocked, a
//! monitor turned off, a display scale changed. Then the saved position points
//! at a place that no longer exists and the window is restored there, mostly
//! or entirely off the visible area -- above the top edge, below the bottom,
//! or on a screen that is gone. It looks random because it depends on which
//! screen the window was last on.
//!
//! So after a position is applied, check that the window's title bar actually
//! lands on a screen. If it does not, put the window in the middle of the
//! screen the mouse is on. A window you can see in the wrong place beats one
//! you cannot find.

use tauri::{PhysicalPosition, PhysicalSize, Runtime, Window};

/// How much of the window's top edge must be on a screen to count as
/// reachable. The title bar is what you grab to move it, so that is what has
/// to be visible: a window whose bottom half is on screen but whose title bar
/// is above the top edge cannot be dragged back.
const GRAB_HEIGHT: i32 = 40;
const GRAB_WIDTH: i32 = 160;

/// Moves `window` onto a screen if none of its title bar is on one.
///
/// Best effort throughout: every failure leaves the window exactly where it
/// was, which is never worse than not checking.
pub fn ensure_on_screen<R: Runtime>(window: &Window<R>) {
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let Ok(monitors) = window.available_monitors() else {
        return;
    };
    if monitors.is_empty() {
        return;
    }

    if title_bar_is_visible(pos, size, monitors.iter().map(|m| (*m.position(), *m.size()))) {
        return;
    }

    // Prefer the screen the window is nearest to, then the cursor's, then the
    // first one there is -- whichever answers.
    let target = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| monitors.into_iter().next());
    let Some(target) = target else {
        return;
    };

    let area_pos = *target.position();
    let area_size = *target.size();
    // Fit inside the screen first, then centre. A window larger than the
    // screen it lands on still gets its title bar at the top where it can be
    // grabbed.
    let width = (size.width as i32).min(area_size.width as i32).max(1);
    let height = (size.height as i32).min(area_size.height as i32).max(1);
    let x = area_pos.x + (area_size.width as i32 - width) / 2;
    let y = area_pos.y + (area_size.height as i32 - height) / 2;

    log::info!(
        "window '{}' was off every screen at ({}, {}); moving it to ({x}, {y})",
        window.label(),
        pos.x,
        pos.y
    );
    let _ = window.set_size(tauri::Size::Physical(PhysicalSize::new(width as u32, height as u32)));
    let _ = window.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
}

/// Whether a grabbable slice of the window's top edge lies on any screen.
///
/// Pure, so it can be tested against made-up screens without a window.
fn title_bar_is_visible(
    pos: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    screens: impl IntoIterator<Item = (PhysicalPosition<i32>, PhysicalSize<u32>)>,
) -> bool {
    // The strip a person would grab: the top GRAB_HEIGHT pixels, full width.
    let bar_left = pos.x;
    let bar_right = pos.x + size.width as i32;
    let bar_top = pos.y;
    let bar_bottom = pos.y + GRAB_HEIGHT;

    screens.into_iter().any(|(spos, ssize)| {
        let s_left = spos.x;
        let s_right = spos.x + ssize.width as i32;
        let s_top = spos.y;
        let s_bottom = spos.y + ssize.height as i32;

        let overlap_w = bar_right.min(s_right) - bar_left.max(s_left);
        let overlap_h = bar_bottom.min(s_bottom) - bar_top.max(s_top);
        overlap_w >= GRAB_WIDTH && overlap_h >= GRAB_HEIGHT
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(x: i32, y: i32, w: u32, h: u32) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
        (PhysicalPosition::new(x, y), PhysicalSize::new(w, h))
    }

    #[test]
    fn a_window_on_the_screen_is_left_alone() {
        let ok = title_bar_is_visible(
            PhysicalPosition::new(100, 100),
            PhysicalSize::new(900, 700),
            [screen(0, 0, 1920, 1080)],
        );
        assert!(ok);
    }

    #[test]
    fn a_window_above_the_top_edge_is_not_reachable() {
        // The classic restore failure: bottom half visible, title bar gone.
        let ok = title_bar_is_visible(
            PhysicalPosition::new(100, -300),
            PhysicalSize::new(900, 700),
            [screen(0, 0, 1920, 1080)],
        );
        assert!(!ok, "a title bar above the screen cannot be grabbed");
    }

    #[test]
    fn a_window_below_the_bottom_edge_is_not_reachable() {
        let ok = title_bar_is_visible(
            PhysicalPosition::new(100, 1070),
            PhysicalSize::new(900, 700),
            [screen(0, 0, 1920, 1080)],
        );
        assert!(!ok);
    }

    #[test]
    fn a_window_on_a_screen_that_is_gone_is_not_reachable() {
        // Saved while docked to a second monitor at x=1920; now undocked.
        let ok = title_bar_is_visible(
            PhysicalPosition::new(2200, 200),
            PhysicalSize::new(900, 700),
            [screen(0, 0, 1920, 1080)],
        );
        assert!(!ok);
    }

    #[test]
    fn a_window_on_a_second_screen_that_exists_is_fine() {
        let ok = title_bar_is_visible(
            PhysicalPosition::new(2200, 200),
            PhysicalSize::new(900, 700),
            [screen(0, 0, 1920, 1080), screen(1920, 0, 2560, 1440)],
        );
        assert!(ok);
    }

    #[test]
    fn a_sliver_at_the_corner_does_not_count_as_reachable() {
        // Ten pixels of title bar peeking in is technically on screen and
        // practically impossible to grab.
        let ok = title_bar_is_visible(
            PhysicalPosition::new(1910, 1070),
            PhysicalSize::new(900, 700),
            [screen(0, 0, 1920, 1080)],
        );
        assert!(!ok);
    }
}
