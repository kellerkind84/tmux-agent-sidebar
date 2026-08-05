---
title: Devin CLI setup
description: Wire up Devin CLI's hooks to report into the sidebar.
---

Devin CLI's hook system is modeled on Claude Code's hook format: command
hooks receive JSON on stdin, and the config shape is the same
`{ "<Trigger>": [{ "matcher": "...", "hooks": [{ "type": "command",
"command": "..." }] }] }` object.

## Config file

Devin CLI reads hook config from (in priority order):

1. `.devin/hooks.v1.json` in the repository root — the whole file **is**
   the hooks object (no `"hooks"` wrapper key).
2. `.devin/config.json` / `.devin/config.local.json` under a `"hooks"` key.
3. `~/.config/devin/config.json` under a `"hooks"` key (user-level).

## Snippet

Create (or merge into) `.devin/hooks.v1.json` in your repository:

```json
{
  "SessionStart": [
    { "matcher": "", "hooks": [{ "type": "command", "command": "~/.tmux/plugins/tmux-agent-sidebar/hook.sh devin session-start" }] }
  ],
  "SessionEnd": [
    { "matcher": "", "hooks": [{ "type": "command", "command": "~/.tmux/plugins/tmux-agent-sidebar/hook.sh devin session-end" }] }
  ],
  "UserPromptSubmit": [
    { "matcher": "", "hooks": [{ "type": "command", "command": "~/.tmux/plugins/tmux-agent-sidebar/hook.sh devin user-prompt-submit" }] }
  ],
  "PermissionRequest": [
    { "matcher": "", "hooks": [{ "type": "command", "command": "~/.tmux/plugins/tmux-agent-sidebar/hook.sh devin permission-denied" }] }
  ],
  "Stop": [
    { "matcher": "", "hooks": [{ "type": "command", "command": "~/.tmux/plugins/tmux-agent-sidebar/hook.sh devin stop" }] }
  ],
  "PostToolUse": [
    { "matcher": "", "hooks": [{ "type": "command", "command": "~/.tmux/plugins/tmux-agent-sidebar/hook.sh devin activity-log" }] }
  ]
}
```

Adjust the `hook.sh` path if your plugin lives somewhere other than
`~/.tmux/plugins/tmux-agent-sidebar`.

## Restart Devin CLI

Restart Devin CLI after adding or editing the hook config so it picks up
the new hooks.
