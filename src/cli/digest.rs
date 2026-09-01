//! `digest` subcommand: an on-demand, human-readable "what happened while
//! I was away" report across every tmux session. Unlike `summary` (a
//! single glanceable line for the status bar), this is meant to be run
//! interactively — e.g. right after attaching to tmux after time away —
//! and prints one line per pane, grouped by [`AttentionBucket`], with the
//! session/window location, how long the pane has been in that state, and
//! (for panes that need you) why.

use crate::time::now_epoch_secs;
use crate::tmux::{self, AttentionBucket, PaneInfo};
use crate::ui::text::{elapsed_label, wait_reason_label};

pub(crate) fn cmd_digest(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let sessions = tmux::query_sessions();
    let entries = collect_entries(&sessions, now_epoch_secs());

    if json {
        println!("{}", render_json(&entries));
    } else {
        print!("{}", render_text(&entries));
    }
    0
}

struct Entry {
    session: String,
    window: String,
    agent: &'static str,
    bucket: AttentionBucket,
    elapsed: String,
    wait_reason: String,
}

fn collect_entries(sessions: &[tmux::SessionInfo], now: u64) -> Vec<Entry> {
    let mut entries = Vec::new();
    for session in sessions {
        for window in &session.windows {
            for pane in &window.panes {
                entries.push(build_entry(&session.session_name, window, pane, now));
            }
        }
    }
    entries
}

fn build_entry(session_name: &str, window: &tmux::WindowInfo, pane: &PaneInfo, now: u64) -> Entry {
    Entry {
        session: session_name.to_string(),
        window: window.window_name.clone(),
        agent: pane.agent.as_str(),
        bucket: pane.attention_bucket(),
        elapsed: elapsed_label(pane.started_at, now),
        wait_reason: wait_reason_label(&pane.wait_reason),
    }
}

fn render_text(entries: &[Entry]) -> String {
    let mut out = String::new();
    render_section(&mut out, "Needs you", AttentionBucket::NeedsYou, entries);
    render_section(&mut out, "Working", AttentionBucket::Working, entries);
    render_section(&mut out, "Idle", AttentionBucket::Idle, entries);
    if out.is_empty() {
        out.push_str("No agent panes found.\n");
    }
    out
}

fn render_section(out: &mut String, title: &str, bucket: AttentionBucket, entries: &[Entry]) {
    let matching: Vec<&Entry> = entries.iter().filter(|e| e.bucket == bucket).collect();
    if matching.is_empty() {
        return;
    }
    out.push_str(&format!("{title} ({}):\n", matching.len()));
    for entry in matching {
        out.push_str(&format!(
            "  {} [{}:{}]",
            entry.agent, entry.session, entry.window
        ));
        if !entry.elapsed.is_empty() {
            out.push_str(&format!(" — {}", entry.elapsed));
        }
        if !entry.wait_reason.is_empty() {
            out.push_str(&format!(" ({})", entry.wait_reason));
        }
        out.push('\n');
    }
    out.push('\n');
}

fn render_json(entries: &[Entry]) -> String {
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "session": e.session,
                "window": e.window,
                "agent": e.agent,
                "bucket": bucket_label(e.bucket),
                "elapsed": e.elapsed,
                "wait_reason": e.wait_reason,
            })
        })
        .collect();
    serde_json::Value::Array(items).to_string()
}

fn bucket_label(bucket: AttentionBucket) -> &'static str {
    match bucket {
        AttentionBucket::NeedsYou => "needs_you",
        AttentionBucket::Working => "working",
        AttentionBucket::Idle => "idle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::{AgentType, PaneStatus, PermissionMode, SessionInfo, WindowInfo};

    fn pane(status: PaneStatus, attention: bool, started_at: Option<u64>) -> PaneInfo {
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
            started_at,
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

    fn session(name: &str, panes: Vec<PaneInfo>) -> SessionInfo {
        SessionInfo {
            session_name: name.into(),
            windows: vec![WindowInfo {
                window_id: "@1".into(),
                window_name: "main".into(),
                window_active: true,
                auto_rename: true,
                panes,
            }],
        }
    }

    #[test]
    fn collect_entries_buckets_and_labels_every_pane() {
        const NOW: u64 = 1_000_000;
        let sessions = vec![session(
            "foo",
            vec![pane(PaneStatus::Running, false, Some(NOW - 65))],
        )];
        let entries = collect_entries(&sessions, NOW);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session, "foo");
        assert_eq!(entries[0].window, "main");
        assert_eq!(entries[0].bucket, AttentionBucket::Working);
        assert_eq!(entries[0].elapsed, "1m5s");
    }

    #[test]
    fn render_text_groups_by_bucket_and_counts() {
        let sessions = vec![session(
            "foo",
            vec![
                pane(PaneStatus::Error, false, None),
                pane(PaneStatus::Idle, false, None),
            ],
        )];
        let entries = collect_entries(&sessions, 0);
        let text = render_text(&entries);
        assert!(text.contains("Needs you (1):"));
        assert!(text.contains("Idle (1):"));
        assert!(!text.contains("Working"));
    }

    #[test]
    fn render_text_reports_when_nothing_found() {
        assert_eq!(render_text(&[]), "No agent panes found.\n");
    }

    #[test]
    fn render_json_round_trips_bucket_labels() {
        let sessions = vec![session("foo", vec![pane(PaneStatus::Error, false, None)])];
        let entries = collect_entries(&sessions, 0);
        let json = render_json(&entries);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed[0]["bucket"], "needs_you");
        assert_eq!(parsed[0]["session"], "foo");
    }
}
