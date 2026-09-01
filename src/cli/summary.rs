//! `summary` subcommand: a one-line, glanceable rollup of every agent pane
//! across every tmux session, meant to be embedded in `status-left` /
//! `status-right` via tmux's `#()` command substitution. Reuses the same
//! `query_sessions()` scan the TUI sidebar runs, so the counts always
//! agree with what the sidebar would show.

use crate::tmux::{self, AttentionBucket};

pub(crate) fn cmd_summary(args: &[String]) -> i32 {
    let plain = args.iter().any(|a| a == "--plain");
    let counts = Counts::collect(&tmux::query_sessions());
    let line = counts.render(plain);
    if !line.is_empty() {
        println!("{line}");
    }
    0
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    needs_you: u32,
    working: u32,
    idle: u32,
}

impl Counts {
    fn collect(sessions: &[tmux::SessionInfo]) -> Self {
        let mut counts = Self::default();
        for pane in sessions
            .iter()
            .flat_map(|s| &s.windows)
            .flat_map(|w| &w.panes)
        {
            match pane.attention_bucket() {
                AttentionBucket::NeedsYou => counts.needs_you += 1,
                AttentionBucket::Working => counts.working += 1,
                AttentionBucket::Idle => counts.idle += 1,
            }
        }
        counts
    }

    /// Render as a compact string, e.g. `⚠ 2 ● 3 ○ 4`. With `plain` false,
    /// each segment is wrapped in a tmux style directive (`#[fg=...]`) so
    /// it colorizes correctly once tmux expands the surrounding
    /// `status-left`/`status-right` format string. Buckets with a zero
    /// count are omitted; an entirely idle/empty board renders as "".
    fn render(&self, plain: bool) -> String {
        let mut segments = Vec::new();
        if self.needs_you > 0 {
            segments.push(segment("⚠", self.needs_you, "colour1", plain));
        }
        if self.working > 0 {
            segments.push(segment("●", self.working, "colour2", plain));
        }
        if self.idle > 0 {
            segments.push(segment("○", self.idle, "colour8", plain));
        }
        segments.join(" ")
    }
}

fn segment(icon: &str, count: u32, color: &str, plain: bool) -> String {
    if plain {
        format!("{icon} {count}")
    } else {
        format!("#[fg={color}]{icon} {count}#[default]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::{AgentType, PaneInfo, PaneStatus, PermissionMode, SessionInfo, WindowInfo};

    fn pane(status: PaneStatus, attention: bool) -> PaneInfo {
        PaneInfo {
            pane_id: "%1".into(),
            pane_active: false,
            status,
            attention,
            agent: AgentType::Claude,
            path: "/tmp".into(),
            current_command: String::new(),
            prompt: String::new(),
            prompt_is_response: false,
            started_at: None,
            wait_reason: String::new(),
            permission_mode: PermissionMode::Default,
            subagents: vec![],
            pane_pid: None,
            worktree: Default::default(),
            session_id: None,
            session_name: String::new(),
            sidebar_spawned: false,
            window_name: String::new(),
            bg_shell_cmd: None,
        }
    }

    fn session_with(panes: Vec<PaneInfo>) -> SessionInfo {
        SessionInfo {
            session_name: "test".into(),
            windows: vec![WindowInfo {
                window_id: "@1".into(),
                window_name: "w".into(),
                window_active: true,
                auto_rename: true,
                panes,
            }],
        }
    }

    #[test]
    fn counts_collect_buckets_every_pane_exactly_once() {
        let sessions = vec![session_with(vec![
            pane(PaneStatus::Running, false),
            pane(PaneStatus::Idle, false),
            pane(PaneStatus::Error, false),
            pane(PaneStatus::Idle, true),
        ])];
        let counts = Counts::collect(&sessions);
        assert_eq!(
            counts,
            Counts {
                needs_you: 2,
                working: 1,
                idle: 1,
            }
        );
    }

    #[test]
    fn counts_collect_empty_for_no_sessions() {
        assert_eq!(Counts::collect(&[]), Counts::default());
    }

    #[test]
    fn render_empty_counts_is_empty_string() {
        assert_eq!(Counts::default().render(true), "");
        assert_eq!(Counts::default().render(false), "");
    }

    #[test]
    fn render_plain_omits_style_directives() {
        let counts = Counts {
            needs_you: 2,
            working: 3,
            idle: 4,
        };
        assert_eq!(counts.render(true), "⚠ 2 ● 3 ○ 4");
    }

    #[test]
    fn render_styled_wraps_each_segment_in_tmux_style_codes() {
        let counts = Counts {
            needs_you: 1,
            working: 0,
            idle: 0,
        };
        assert_eq!(counts.render(false), "#[fg=colour1]⚠ 1#[default]");
    }

    #[test]
    fn render_skips_zero_buckets() {
        let counts = Counts {
            needs_you: 0,
            working: 5,
            idle: 0,
        };
        assert_eq!(counts.render(true), "● 5");
    }
}
