//! OLED screensaver: the progress bar is only drawn while a transfer runs.

use vita_syncthing_app::domain::AppState;
use vita_syncthing_app::ui::gfx::{rgb, Gfx, GFX_W};
use vita_syncthing_app::ui::screens::{render_oled_screensaver, SAVER_W};
use vita_syncthing_app::ui::theme::UiTheme;

/// Renders the screensaver and tells whether anything is drawn where the progress bar goes.
fn progress_bar_drawn(state: &AppState) -> bool {
    let mut gfx = Gfx::new();
    render_oled_screensaver(&mut gfx, &UiTheme::for_mode(state.theme_mode), state);
    let (bx, by) = (state.saver_x as i32, state.saver_y as i32);
    let black = rgb(0, 0, 0);
    (by + 104..by + 112).any(|y| {
        (bx + 2..bx + SAVER_W as i32 - 6).any(|x| gfx.pixels[(y * GFX_W + x) as usize] != black)
    })
}

#[test]
fn test_progress_bar_only_while_transferring() {
    let mut state = AppState::new(Vec::new(), Vec::new(), "/tmp");

    // Idle (no connected session) and Ready (connected, up to date): no bar.
    assert!(!progress_bar_drawn(&state));
    state.session_connected = true;
    assert!(!progress_bar_drawn(&state));

    // Uploading: indeterminate bar.
    state.uploading = true;
    assert!(progress_bar_drawn(&state));

    // Downloading: determinate bar, even at 0 %.
    state.uploading = false;
    state.sync_in_progress = true;
    state.progress_pct = Some(0);
    assert!(progress_bar_drawn(&state));
}
