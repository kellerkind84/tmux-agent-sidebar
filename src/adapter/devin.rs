use crate::event::{AgentEvent, AgentEventKind, EventAdapter};
use crate::tmux::DEVIN_AGENT;
use serde_json::Value;

use super::{HookRegistration, json_str, json_value_or_null, optional_str};

pub struct DevinAdapter;

impl DevinAdapter {
    /// Single source of truth for Devin CLI hook wiring. Devin CLI's hook
    /// system is deliberately modeled on Claude Code's hook format
    /// (command hooks receive JSON on stdin, config files use the same
    /// `{ trigger: [{ matcher, hooks: [{ type: "command", command }] }] }`
    /// shape), but Devin CLI does not have Claude-style subagents, so there
    /// is no `SubagentStart`/`SubagentStop` equivalent.
    ///
    /// Devin CLI's hook event enum: `PreToolUse`, `PostToolUse`,
    /// `PermissionRequest`, `UserPromptSubmit`, `Stop`, `PostCompaction`,
    /// `SessionStart`, `SessionEnd`.
    ///
    /// Caveats:
    /// - `PreToolUse` is supported by Devin CLI but not yet wired — no
    ///   existing `AgentEventKind` maps cleanly onto a "before the tool
    ///   runs" moment without also modeling permission decisions, and
    ///   `PostToolUse` already covers the activity log. Same rationale the
    ///   Codex adapter uses for leaving its `PreToolUse` trigger unwired.
    /// - `PostCompaction` is not wired — there is no existing
    ///   `AgentEventKind` for compaction/summary events, and none of the
    ///   other adapters (Claude included) model one either.
    /// - `PermissionRequest` maps to `AgentEventKind::PermissionDenied`.
    ///   This is the closest existing kind (a permission-related pane
    ///   status/attention update); Devin's hook fires when a permission
    ///   *decision is needed* rather than only on denial, but reusing the
    ///   existing kind avoids introducing a near-duplicate variant.
    pub const HOOK_REGISTRATIONS: &'static [HookRegistration] = &[
        HookRegistration {
            trigger: "SessionStart",
            matcher: None,
            kind: AgentEventKind::SessionStart,
        },
        HookRegistration {
            trigger: "SessionEnd",
            matcher: None,
            kind: AgentEventKind::SessionEnd,
        },
        HookRegistration {
            trigger: "UserPromptSubmit",
            matcher: None,
            kind: AgentEventKind::UserPromptSubmit,
        },
        HookRegistration {
            trigger: "PermissionRequest",
            matcher: None,
            kind: AgentEventKind::PermissionDenied,
        },
        HookRegistration {
            trigger: "Stop",
            matcher: None,
            kind: AgentEventKind::Stop,
        },
        HookRegistration {
            trigger: "PostToolUse",
            matcher: None,
            kind: AgentEventKind::ActivityLog,
        },
    ];
}

impl EventAdapter for DevinAdapter {
    fn parse(&self, event_name: &str, input: &Value) -> Option<AgentEvent> {
        match event_name {
            "session-start" => Some(AgentEvent::SessionStart {
                agent: DEVIN_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                source: json_str(input, "source").into(),
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "session-end" => Some(AgentEvent::SessionEnd {
                end_reason: json_str(input, "reason").into(),
            }),
            "user-prompt-submit" => Some(AgentEvent::UserPromptSubmit {
                agent: DEVIN_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                prompt: json_str(input, "prompt").into(),
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "permission-denied" => Some(AgentEvent::PermissionDenied {
                agent: DEVIN_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "stop" => Some(AgentEvent::Stop {
                agent: DEVIN_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                last_message: String::new(),
                response: None,
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "activity-log" => {
                let tool_name = json_str(input, "tool_name");
                if tool_name.is_empty() {
                    return None;
                }
                Some(AgentEvent::ActivityLog {
                    tool_name: tool_name.into(),
                    tool_input: json_value_or_null(input, "tool_input"),
                    tool_response: json_value_or_null(input, "tool_response"),
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hook_registrations_match_parse_arms() {
        super::super::assert_table_drift_free("devin", DevinAdapter::HOOK_REGISTRATIONS);
    }

    #[test]
    fn session_start() {
        let adapter = DevinAdapter;
        let input = json!({"cwd": "/home/user", "session_id": "sess-devin-1"});
        let event = adapter.parse("session-start", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionStart {
                agent: DEVIN_AGENT.into(),
                cwd: "/home/user".into(),
                permission_mode: "".into(),
                source: "".into(),
                worktree: None,
                agent_id: None,
                session_id: Some("sess-devin-1".into()),
            }
        );
    }

    #[test]
    fn session_start_captures_source() {
        let adapter = DevinAdapter;
        let input = json!({"cwd": "/tmp", "source": "startup"});
        let event = adapter.parse("session-start", &input).unwrap();
        match event {
            AgentEvent::SessionStart { source, .. } => assert_eq!(source, "startup"),
            other => panic!("expected SessionStart, got {other:?}"),
        }
    }

    #[test]
    fn session_start_missing_fields_default_to_empty() {
        let adapter = DevinAdapter;
        let event = adapter.parse("session-start", &json!({})).unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionStart {
                agent: "devin".into(),
                cwd: "".into(),
                permission_mode: "".into(),
                source: "".into(),
                worktree: None,
                agent_id: None,
                session_id: None,
            }
        );
    }

    #[test]
    fn session_end_captures_reason() {
        let adapter = DevinAdapter;
        let event = adapter
            .parse("session-end", &json!({"reason": "user_exit"}))
            .unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionEnd {
                end_reason: "user_exit".into()
            }
        );
    }

    #[test]
    fn session_end_missing_reason_defaults_to_empty() {
        let adapter = DevinAdapter;
        assert_eq!(
            adapter.parse("session-end", &json!({})).unwrap(),
            AgentEvent::SessionEnd {
                end_reason: "".into()
            }
        );
    }

    #[test]
    fn user_prompt_submit() {
        let adapter = DevinAdapter;
        let input = json!({"cwd": "/tmp", "prompt": "fix the bug", "session_id": "sess-devin-2"});
        let event = adapter.parse("user-prompt-submit", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::UserPromptSubmit {
                agent: DEVIN_AGENT.into(),
                cwd: "/tmp".into(),
                permission_mode: "".into(),
                prompt: "fix the bug".into(),
                worktree: None,
                agent_id: None,
                session_id: Some("sess-devin-2".into()),
            }
        );
    }

    #[test]
    fn permission_denied_round_trip() {
        let adapter = DevinAdapter;
        let input = json!({
            "cwd": "/tmp",
            "tool_name": "Bash",
            "tool_input": {"command": "rm -rf /"},
            "session_id": "sess-devin-3"
        });
        let event = adapter.parse("permission-denied", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::PermissionDenied {
                agent: DEVIN_AGENT.into(),
                cwd: "/tmp".into(),
                permission_mode: "".into(),
                worktree: None,
                agent_id: None,
                session_id: Some("sess-devin-3".into()),
            }
        );
    }

    #[test]
    fn permission_denied_missing_fields_default_to_empty() {
        let adapter = DevinAdapter;
        assert_eq!(
            adapter.parse("permission-denied", &json!({})).unwrap(),
            AgentEvent::PermissionDenied {
                agent: "devin".into(),
                cwd: "".into(),
                permission_mode: "".into(),
                worktree: None,
                agent_id: None,
                session_id: None,
            }
        );
    }

    #[test]
    fn stop_has_no_response() {
        let adapter = DevinAdapter;
        let input = json!({"cwd": "/tmp", "stop_hook_active": false, "session_id": "sess-devin-4"});
        let event = adapter.parse("stop", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::Stop {
                agent: DEVIN_AGENT.into(),
                cwd: "/tmp".into(),
                permission_mode: "".into(),
                last_message: "".into(),
                response: None,
                worktree: None,
                agent_id: None,
                session_id: Some("sess-devin-4".into()),
            }
        );
    }

    #[test]
    fn activity_log_from_post_tool_use() {
        let adapter = DevinAdapter;
        let input = json!({
            "tool_name": "Bash",
            "tool_input": {"command": "ls -la"},
            "tool_response": {"success": true, "output": "file.txt\n", "error": null}
        });
        let event = adapter.parse("activity-log", &input).unwrap();
        match event {
            AgentEvent::ActivityLog {
                tool_name,
                tool_input,
                tool_response,
            } => {
                assert_eq!(tool_name, "Bash");
                assert_eq!(
                    tool_input.get("command").and_then(|v| v.as_str()),
                    Some("ls -la")
                );
                assert_eq!(tool_response.get("success"), Some(&json!(true)));
            }
            other => panic!("expected ActivityLog, got {other:?}"),
        }
    }

    #[test]
    fn activity_log_empty_tool_name_rejected() {
        let adapter = DevinAdapter;
        assert!(adapter.parse("activity-log", &json!({})).is_none());
    }

    #[test]
    fn pre_tool_use_not_supported() {
        // PreToolUse is a real Devin CLI trigger but is not yet wired to
        // an internal AgentEventKind (see HOOK_REGISTRATIONS doc comment).
        assert!(DevinAdapter.parse("pre-tool-use", &json!({})).is_none());
    }

    #[test]
    fn post_compaction_not_supported() {
        // Devin's PostCompaction has no existing AgentEventKind equivalent.
        assert!(
            DevinAdapter
                .parse("post-compaction", &json!({"summary": "compacted"}))
                .is_none()
        );
    }

    #[test]
    fn subagent_events_not_supported() {
        // Devin CLI does not emit SubagentStart/SubagentStop.
        assert!(
            DevinAdapter
                .parse("subagent-start", &json!({"agent_type": "X"}))
                .is_none()
        );
        assert!(
            DevinAdapter
                .parse("subagent-stop", &json!({"agent_type": "X"}))
                .is_none()
        );
    }

    #[test]
    fn cwd_changed_not_supported() {
        assert!(DevinAdapter.parse("cwd-changed", &json!({})).is_none());
    }

    #[test]
    fn unknown_event_ignored() {
        assert!(DevinAdapter.parse("something-else", &json!({})).is_none());
    }
}
