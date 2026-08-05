---
title: Hermes Agent setup
description: Wire up Hermes Agent's shell hooks to report into the sidebar.
---

Hermes Agent ([NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent))
has a "Shell Hooks" system: declare a `hooks:` block in
`~/.hermes/config.yaml` and Hermes spawns your command as a subprocess
whenever the matching event fires, piping a JSON payload to its stdin.

This is distinct from Hermes's separate Python-only "Gateway hooks" and
"Plugin hooks" systems — those require authoring a Python handler and are
not relevant to a plain shell-command integration like this one.

## Config file

Hermes reads shell-hook config from `~/.hermes/config.yaml` under a
`hooks:` key.

## Snippet

Merge this into `~/.hermes/config.yaml`:

```yaml
hooks:
  on_session_start:
    - command: "~/.tmux/plugins/tmux-agent-sidebar/hook.sh hermes session-start"
  on_session_end:
    - command: "~/.tmux/plugins/tmux-agent-sidebar/hook.sh hermes session-end"
  pre_llm_call:
    - command: "~/.tmux/plugins/tmux-agent-sidebar/hook.sh hermes user-prompt-submit"
  subagent_stop:
    - command: "~/.tmux/plugins/tmux-agent-sidebar/hook.sh hermes subagent-stop"
  post_tool_call:
    - command: "~/.tmux/plugins/tmux-agent-sidebar/hook.sh hermes activity-log"

hooks_auto_accept: true
```

Adjust the `hook.sh` path if your plugin lives somewhere other than
`~/.tmux/plugins/tmux-agent-sidebar`.

`pre_llm_call` is Hermes's stand-in for Claude Code's `UserPromptSubmit` —
per Hermes's own docs, it "fires at the same place and already supports
context injection", so there is no separate prompt-submit event to wire.

### Consent

Each unique `(event, command)` pair prompts for approval the first time
Hermes sees it (interactively), then persists the decision to
`~/.hermes/shell-hooks-allowlist.json`. Setting `hooks_auto_accept: true`
(shown above) — or passing `--accept-hooks` / setting
`HERMES_ACCEPT_HOOKS=1` — skips that prompt, which matters for the sidebar
hooks since they fire silently in the background.

## Restart Hermes

Restart `hermes` (or the gateway process, if you run one) after editing
`~/.hermes/config.yaml` so it picks up the new hooks.
