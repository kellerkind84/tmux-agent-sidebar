use crate::event::{AgentEvent, AgentEventKind, EventAdapter};
use crate::tmux::HERMES_AGENT;
use serde_json::Value;

use super::{HookRegistration, json_str, json_value_or_null, optional_str};

pub struct HermesAdapter;

/// Read a string field nested under the Hermes wire protocol's `extra`
/// object, falling back to an empty string when absent.
///
/// Hermes Agent's shell-hook stdin payload only ever puts
/// `hook_event_name`/`tool_name`/`tool_input`/`session_id`/`cwd` at the top
/// level; every event-specific field (the kwargs documented per hook in the
/// upstream "Hook Reference", e.g. `child_summary`, `user_message`,
/// `completed`) is nested under `extra`. See
/// <https://hermes-agent.nousresearch.com/docs/user-guide/features/hooks>.
fn extra_str<'a>(input: &'a Value, key: &str) -> &'a str {
    input
        .get("extra")
        .and_then(|extra| extra.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

fn extra_bool(input: &Value, key: &str) -> bool {
    input
        .get("extra")
        .and_then(|extra| extra.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn extra_value(input: &Value, key: &str) -> Value {
    input
        .get("extra")
        .and_then(|extra| extra.get(key))
        .cloned()
        .unwrap_or(Value::Null)
}

impl HermesAdapter {
    /// Single source of truth for Hermes Agent's "Shell Hooks" wiring
    /// (`hooks:` block in `~/.hermes/config.yaml`; not to be confused with
    /// Hermes's separate Python-only "Gateway hooks" or "Plugin hooks"
    /// systems, which are out of scope for a shell-command integration).
    ///
    /// Every Hermes shell-hook event shares one stdin payload shape,
    /// regardless of which event fired: `{ hook_event_name, tool_name,
    /// tool_input, session_id, cwd, extra }`. `tool_name`/`tool_input` are
    /// `null` for non-tool events; `extra` carries every event-specific
    /// kwarg (see [`extra_str`]).
    ///
    /// Hermes's `VALID_HOOKS` event set (subset relevant here):
    /// `on_session_start`, `on_session_end`, `pre_tool_call`,
    /// `post_tool_call`, `pre_llm_call`, `post_llm_call`, `pre_verify`,
    /// `subagent_start`, `subagent_stop`, plus gateway-only/Kanban-only
    /// events (`on_session_finalize`, `on_session_reset`,
    /// `pre_gateway_dispatch`, `kanban_task_blocked`, ...).
    ///
    /// Caveats:
    /// - `pre_tool_call` is supported by Hermes but not wired — it is a
    ///   blocking policy hook (a shell hook can reject the tool call by
    ///   writing `{"decision": "block", ...}` to stdout), not a
    ///   state-reporting one. Wiring it would require the sidebar to
    ///   answer on stdout to avoid breaking the allow/block flow, which is
    ///   out of scope for an observer. Same rationale Codex/Devin/Copilot
    ///   use for their own pre-tool-use triggers.
    /// - `pre_llm_call` is Hermes's explicit stand-in for Claude Code's
    ///   `UserPromptSubmit` (the upstream docs say so directly: "Claude
    ///   Code's `UserPromptSubmit` event is intentionally not a separate
    ///   Hermes event — `pre_llm_call` fires at the same place and already
    ///   supports context injection"). Mapped to
    ///   `AgentEventKind::UserPromptSubmit` here. Like `pre_tool_call`,
    ///   `pre_llm_call` also supports a blocking-ish response (context
    ///   injection via `{"context": "..."}`), but writing nothing to
    ///   stdout is always safe (no injection), unlike a `pre_tool_call`
    ///   block — so it's safe to observe here.
    /// - `subagent_start` is a real Hermes event (documented as a plugin
    ///   hook, and listed alongside `subagent_stop` in the outbound-webhook
    ///   event set), but there is no confirmation it is *independently*
    ///   subscribable as a shell hook the way `subagent_stop` explicitly
    ///   is (the shell-vs-plugin-hook comparison table calls out
    ///   `subagent_stop` by name, not `subagent_start`). Left unwired to
    ///   avoid guessing at an unconfirmed trigger name.
    /// - `post_llm_call`, `pre_verify`, `on_session_finalize`,
    ///   `on_session_reset`, `pre_gateway_dispatch`, `kanban_task_blocked`,
    ///   and other gateway/Kanban-only events have no clear sidebar-facing
    ///   equivalent and are left unwired.
    /// - `subagent_stop`'s callback signature is `(parent_session_id,
    ///   child_role, child_summary, child_status, tool_call_history,
    ///   duration_ms)` — there is no "agent_type" field the way Claude
    ///   Code's subagent hooks have one. `child_role` (e.g.
    ///   `"leaf"`/`"orchestrator"`, only populated when Hermes's
    ///   orchestrator-role feature is enabled) is the closest analog, and
    ///   `child_summary` ("the final response the child returned to the
    ///   parent") is the closest analog to `last_message`. There is no
    ///   subagent transcript-file equivalent, so `transcript_path` is
    ///   always empty.
    pub const HOOK_REGISTRATIONS: &'static [HookRegistration] = &[
        HookRegistration {
            trigger: "on_session_start",
            matcher: None,
            kind: AgentEventKind::SessionStart,
        },
        HookRegistration {
            trigger: "on_session_end",
            matcher: None,
            kind: AgentEventKind::SessionEnd,
        },
        HookRegistration {
            trigger: "pre_llm_call",
            matcher: None,
            kind: AgentEventKind::UserPromptSubmit,
        },
        HookRegistration {
            trigger: "subagent_stop",
            matcher: None,
            kind: AgentEventKind::SubagentStop,
        },
        HookRegistration {
            trigger: "post_tool_call",
            matcher: None,
            kind: AgentEventKind::ActivityLog,
        },
    ];
}

impl EventAdapter for HermesAdapter {
    fn parse(&self, event_name: &str, input: &Value) -> Option<AgentEvent> {
        match event_name {
            "session-start" => Some(AgentEvent::SessionStart {
                agent: HERMES_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                // on_session_start fires exactly once, only for a
                // brand-new session (never on continuation), so "startup"
                // is always accurate -- Hermes has no separate
                // resume/source field on this event.
                source: "startup".into(),
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "session-end" => {
                let end_reason = if extra_bool(input, "interrupted") {
                    "interrupted"
                } else if extra_bool(input, "completed") {
                    "completed"
                } else {
                    "failed"
                };
                Some(AgentEvent::SessionEnd {
                    end_reason: end_reason.into(),
                })
            }
            "user-prompt-submit" => Some(AgentEvent::UserPromptSubmit {
                agent: HERMES_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                prompt: extra_str(input, "user_message").into(),
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
                    tool_response: extra_value(input, "result"),
                })
            }
            "subagent-stop" => {
                // Real Hermes payloads carry the closest analog under
                // `extra.child_role`; a top-level `agent_type` is also
                // accepted so this stays compatible with the generic
                // `minimal_payload()` fixture used by
                // `assert_table_drift_free` (shared across all adapters).
                let mut agent_type = json_str(input, "agent_type");
                if agent_type.is_empty() {
                    agent_type = extra_str(input, "child_role");
                }
                Some(AgentEvent::SubagentStop {
                    agent_type: agent_type.into(),
                    // subagent_stop's kwargs carry no child-subagent id
                    // (only subagent_start's do, and that trigger is
                    // unwired here -- see HOOK_REGISTRATIONS doc comment).
                    agent_id: None,
                    last_message: extra_str(input, "child_summary").into(),
                    transcript_path: String::new(),
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
        super::super::assert_table_drift_free("hermes", HermesAdapter::HOOK_REGISTRATIONS);
    }

    #[test]
    fn session_start() {
        let adapter = HermesAdapter;
        let input = json!({"cwd": "/home/user", "session_id": "sess_abc123"});
        let event = adapter.parse("session-start", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionStart {
                agent: HERMES_AGENT.into(),
                cwd: "/home/user".into(),
                permission_mode: "".into(),
                source: "startup".into(),
                worktree: None,
                agent_id: None,
                session_id: Some("sess_abc123".into()),
            }
        );
    }

    #[test]
    fn session_start_missing_fields_default_to_empty() {
        let adapter = HermesAdapter;
        let event = adapter.parse("session-start", &json!({})).unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionStart {
                agent: "hermes".into(),
                cwd: "".into(),
                permission_mode: "".into(),
                source: "startup".into(),
                worktree: None,
                agent_id: None,
                session_id: None,
            }
        );
    }

    #[test]
    fn session_end_completed() {
        let adapter = HermesAdapter;
        let input = json!({"extra": {"completed": true, "interrupted": false}});
        let event = adapter.parse("session-end", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionEnd {
                end_reason: "completed".into()
            }
        );
    }

    #[test]
    fn session_end_interrupted_takes_priority() {
        let adapter = HermesAdapter;
        let input = json!({"extra": {"completed": true, "interrupted": true}});
        let event = adapter.parse("session-end", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionEnd {
                end_reason: "interrupted".into()
            }
        );
    }

    #[test]
    fn session_end_missing_fields_defaults_to_failed() {
        let adapter = HermesAdapter;
        let event = adapter.parse("session-end", &json!({})).unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionEnd {
                end_reason: "failed".into()
            }
        );
    }

    #[test]
    fn user_prompt_submit_from_pre_llm_call() {
        let adapter = HermesAdapter;
        let input = json!({
            "cwd": "/tmp",
            "session_id": "sess_abc123",
            "extra": {"user_message": "fix the bug", "is_first_turn": true}
        });
        let event = adapter.parse("user-prompt-submit", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::UserPromptSubmit {
                agent: HERMES_AGENT.into(),
                cwd: "/tmp".into(),
                permission_mode: "".into(),
                prompt: "fix the bug".into(),
                worktree: None,
                agent_id: None,
                session_id: Some("sess_abc123".into()),
            }
        );
    }

    #[test]
    fn activity_log_from_post_tool_call() {
        let adapter = HermesAdapter;
        let input = json!({
            "tool_name": "terminal",
            "tool_input": {"command": "ls -la"},
            "session_id": "sess_abc123",
            "cwd": "/tmp",
            "extra": {"result": "{\"output\": \"file.txt\\n\"}", "task_id": "t1", "duration_ms": 12}
        });
        let event = adapter.parse("activity-log", &input).unwrap();
        match event {
            AgentEvent::ActivityLog {
                tool_name,
                tool_input,
                tool_response,
            } => {
                assert_eq!(tool_name, "terminal");
                assert_eq!(
                    tool_input.get("command").and_then(|v| v.as_str()),
                    Some("ls -la")
                );
                assert_eq!(
                    tool_response.as_str(),
                    Some("{\"output\": \"file.txt\\n\"}")
                );
            }
            other => panic!("expected ActivityLog, got {other:?}"),
        }
    }

    #[test]
    fn activity_log_empty_tool_name_rejected() {
        let adapter = HermesAdapter;
        assert!(adapter.parse("activity-log", &json!({})).is_none());
    }

    #[test]
    fn subagent_stop_round_trip() {
        let adapter = HermesAdapter;
        let input = json!({
            "session_id": "sess_parent",
            "extra": {
                "child_role": "leaf",
                "child_summary": "Found the bug at main.rs:42",
                "child_status": "completed",
                "duration_ms": 4200
            }
        });
        let event = adapter.parse("subagent-stop", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::SubagentStop {
                agent_type: "leaf".into(),
                agent_id: None,
                last_message: "Found the bug at main.rs:42".into(),
                transcript_path: "".into(),
            }
        );
    }

    #[test]
    fn subagent_stop_missing_child_role_defaults_to_empty() {
        // child_role is only populated when Hermes's orchestrator-role
        // feature is enabled; a real subagent_stop event without it should
        // still be accepted (it's an observer event, not gated on this).
        let adapter = HermesAdapter;
        let input = json!({"extra": {"child_summary": "done", "child_status": "completed"}});
        let event = adapter.parse("subagent-stop", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::SubagentStop {
                agent_type: "".into(),
                agent_id: None,
                last_message: "done".into(),
                transcript_path: "".into(),
            }
        );
    }

    #[test]
    fn pre_tool_call_not_supported() {
        // pre_tool_call is a real Hermes trigger but is not wired -- see
        // HOOK_REGISTRATIONS doc comment.
        assert!(HermesAdapter.parse("pre-tool-use", &json!({})).is_none());
    }

    #[test]
    fn subagent_start_not_supported() {
        assert!(
            HermesAdapter
                .parse("subagent-start", &json!({"agent_type": "X"}))
                .is_none()
        );
    }

    #[test]
    fn permission_denied_not_supported() {
        assert!(
            HermesAdapter
                .parse("permission-denied", &json!({}))
                .is_none()
        );
    }

    #[test]
    fn cwd_changed_not_supported() {
        assert!(HermesAdapter.parse("cwd-changed", &json!({})).is_none());
    }

    #[test]
    fn unknown_event_ignored() {
        assert!(HermesAdapter.parse("something-else", &json!({})).is_none());
    }
}
