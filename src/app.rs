//! Main application orchestration: prime the [`AppState`], spawn background
//! workers, and run the crossterm event loop. Split out from `src/main.rs` so
//! the binary entry point only handles CLI arg parsing, signal wiring, and
//! TUI session setup.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{self};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::SPINNER_PULSE;
use crate::state::BottomTab;
use crate::tmux;

/// How often the event loop confirms its own pane still exists (see
/// [`is_orphaned`] for why this exists at all).
const ORPHAN_CHECK_INTERVAL: Duration = Duration::from_secs(30);

mod input;
mod render;
mod setup;
mod workers;

/// Run the TUI event loop. Returns when the loop exits (currently only on
/// fatal I/O error, since the loop is `loop { ... }`).
///
/// `needs_refresh` is the process-wide SIGUSR1 flag owned by `main.rs` — the
/// signal handler must reference a static visible at signal-handler time,
/// so the static stays with the `extern "C"` handler in the binary crate and
/// we just borrow it here.
pub fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    tmux_pane: String,
    needs_refresh: &'static AtomicBool,
) -> io::Result<()> {
    let mut state = setup::init_state(tmux_pane);
    let mut window_inactive_count: u32 = 0;

    let workers = workers::spawn(&state);
    let workers::Workers {
        git_rx,
        session_rx,
        version_rx,
        git_tab_active,
    } = workers;

    let mut last_refresh = std::time::Instant::now();
    let mut last_spinner = std::time::Instant::now();
    let mut last_orphan_check = std::time::Instant::now();
    let refresh_interval = Duration::from_secs(1);
    let spinner_interval = Duration::from_millis(200);
    let mut needs_redraw = true;

    loop {
        if last_orphan_check.elapsed() >= ORPHAN_CHECK_INTERVAL {
            if is_orphaned(&state.tmux_pane) {
                // Our own pane no longer exists (killed without the process
                // ever receiving a terminating signal — e.g. a forced
                // kill-window, a tmux server restart, or a crash that left
                // us reparented). Looping forever from here on would just
                // burn CPU polling tmux/git/ps for a pane nobody can see;
                // exit instead of accumulating as a permanent orphan.
                return Ok(());
            }
            last_orphan_check = std::time::Instant::now();
        }

        if needs_redraw {
            render::render_frame(terminal, &mut state)?;
            needs_redraw = false;
        }

        let refresh_timeout = refresh_interval.saturating_sub(last_refresh.elapsed());
        let spinner_timeout = spinner_interval.saturating_sub(last_spinner.elapsed());
        let timeout = if needs_refresh.load(Ordering::Relaxed) {
            Duration::ZERO
        } else {
            refresh_timeout
                .min(spinner_timeout)
                .min(Duration::from_millis(16))
        };
        if event::poll(timeout)? {
            loop {
                let ev = event::read()?;
                if input::handle_event(ev, &mut state, &git_tab_active, terminal) {
                    needs_redraw = true;
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        if last_spinner.elapsed() >= spinner_interval {
            state.spinner_frame = (state.spinner_frame + 1) % SPINNER_PULSE.len();
            if state.pet_enabled {
                let term_width = terminal.size().map(|s| s.width).unwrap_or(60);
                state.tick_pet(term_width);
            }
            last_spinner = std::time::Instant::now();
            needs_redraw = true;
        }

        let sigusr1 = needs_refresh.swap(false, Ordering::Relaxed);
        if sigusr1 || last_refresh.elapsed() >= refresh_interval {
            let previous_focused_pane_id = state.focus_state.focused_pane_id.clone();
            let is_window_active = state.refresh();
            if state.focus_state.focused_pane_id != previous_focused_pane_id {
                render::refresh_git_for_focused_pane(&mut state);
            }
            needs_redraw = true;
            if is_window_active {
                if window_inactive_count >= 2 {
                    state.global.load_from_tmux();
                    state.rebuild_row_targets();
                }
                window_inactive_count = 0;
            } else {
                window_inactive_count = window_inactive_count.saturating_add(1);
            }
            git_tab_active.store(state.bottom_tab == BottomTab::GitStatus, Ordering::Relaxed);
            last_refresh = std::time::Instant::now();
        }

        if let Ok(data) = git_rx.try_recv() {
            state.apply_git_data(data);
            needs_redraw = true;
        }

        if let Ok(names) = session_rx.try_recv() {
            state.sessions.names = names;
            state.sessions.dirty = true;
            needs_redraw = true;
        }

        if let Ok(notice) = version_rx.try_recv() {
            state.version_notice = Some(notice);
            needs_redraw = true;
        }

        state
            .global
            .flush_pending_cursor_save(std::time::Duration::from_millis(120));
    }
}

/// Whether this process should stop running: either `tmux_pane` no longer
/// exists on the server this process talks to, or it exists but has been
/// silently handed to a *different* sidebar instance (e.g. `respawn-pane`
/// replacing the foreground command without ever signaling the old one —
/// pane existence alone wouldn't catch that, since the pane id is still
/// valid). `@sidebar_pid` is the pane-scoped marker `main.rs` writes with
/// its own pid on startup, so whichever instance is actually running in
/// the pane right now is the one whose pid is currently stored there.
///
/// Orphaned sidebar instances are a real failure mode, not a hypothetical
/// one: a pane can be killed or repurposed in ways that don't deliver a
/// signal this process would act on (forced `kill-window`/`kill-session`,
/// `respawn-pane`, the tmux server itself restarting or crashing, etc.),
/// and the event loop has no other exit condition — without this check
/// such an instance would poll tmux/git/ps forever, one more permanent
/// background process per incident, indefinitely compounding system load
/// over time.
fn is_orphaned(tmux_pane: &str) -> bool {
    if tmux::pane_session_name(tmux_pane).is_none() {
        return true;
    }
    let recorded_pid = tmux::get_pane_option_value(tmux_pane, tmux::SIDEBAR_PID);
    !recorded_pid.is_empty() && recorded_pid != std::process::id().to_string()
}
