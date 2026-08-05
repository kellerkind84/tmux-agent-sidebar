---
title: Hermes Agent
description: What the sidebar shows for Hermes Agent panes, and what is not available due to the Hermes shell-hook payload shape.
---

Hermes Agent's shell hooks share a single uniform stdin payload shape
across every event (`{ hook_event_name, tool_name, tool_input, session_id,
cwd, extra }`), unlike Claude Code / Devin CLI / Copilot CLI, which each
have a distinct field set per event. Event-specific data lives inside the
`extra` object.

## What you get

### Status and prompts

- Live status from `on_session_start` / `pre_llm_call` (Hermes's
  `UserPromptSubmit` equivalent) / `subagent_stop`
- Prompt text from `pre_llm_call`'s `extra.user_message`
- Elapsed time since the last prompt

### Sub-agent tree

- Subagent completion from `subagent_stop` — `extra.child_role` (only
  populated when Hermes's orchestrator-role feature is enabled) stands in
  for an "agent type", and `extra.child_summary` stands in for the last
  message

### Activity log

- Tool calls recorded from `post_tool_call` (`tool_name` / `tool_input` are
  top-level; the result is `extra.result`)

### Git

- Branch display from the pane's `cwd`

## What is not available

| Feature                    | Why |
| --------------------------- | --- |
| Permission badge            | Hermes's shell-hook payload does not include a permission-mode field |
| Response text display       | `on_session_end` carries no final assistant-message text |
| Background shell state      | `post_tool_call`'s payload does not document a background-shell flag |
| Task progress counter       | Hermes does not emit task-lifecycle shell-hook events |
| Worktree lifecycle tracking | Hermes does not emit worktree-create/remove shell-hook events |
| Blocking/permission events  | `pre_tool_call` is a blocking policy hook, not a state-reporting one, so it is intentionally left unwired (see the setup guide) |

## Setup

Wire the hooks in `~/.hermes/config.yaml` — see
[Hermes Agent setup](/tmux-agent-sidebar/getting-started/hermes/).
