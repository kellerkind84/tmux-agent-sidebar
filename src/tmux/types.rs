pub const CLAUDE_AGENT: &str = "claude";
pub const CODEX_AGENT: &str = "codex";
pub const OPENCODE_AGENT: &str = "opencode";
pub const DEVIN_AGENT: &str = "devin";
pub const COPILOT_AGENT: &str = "copilot";
pub const HERMES_AGENT: &str = "hermes";

#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub pane_id: String,
    pub pane_active: bool,
    pub status: PaneStatus,
    pub attention: bool,
    pub agent: AgentType,
    pub path: String,
    pub current_command: String,
    pub prompt: String,
    pub prompt_is_response: bool,
    pub started_at: Option<u64>,
    pub wait_reason: String,
    pub permission_mode: PermissionMode,
    pub subagents: Vec<String>,
    pub pane_pid: Option<u32>,
    pub worktree: WorktreeMetadata,
    pub session_id: Option<String>,
    pub session_name: String,
    /// `true` when the window this pane lives in was created by the
    /// sidebar's spawn flow (via the `@agent-sidebar-spawned` window
    /// option). Used by the row renderer to show a clickable red `×`
    /// in place of the usual `+` worktree marker.
    pub sidebar_spawned: bool,
    /// Most recent backgrounded Bash command, if any. Populated while the
    /// pane status is `Background` (or `Running` with a backgrounded shell
    /// still alive) so the row body can surface the actual command.
    pub bg_shell_cmd: Option<String>,
    /// Tmux window name this pane lives in. Populated for every pane
    /// (agent or plain) by `build_session_hierarchy`; used to label
    /// lightweight, non-agent rows shown when `@sidebar_show_windows`
    /// is enabled (see `crate::ui::panes::row::render_plain_pane_line`).
    pub window_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct WorktreeMetadata {
    pub name: String,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaneStatus {
    Running,
    Background,
    Waiting,
    Idle,
    Error,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionMode {
    Default,
    Plan,
    AcceptEdits,
    Auto,
    DontAsk,
    BypassPermissions,
    Defer,
}

impl PermissionMode {
    /// Parse the permission-mode label written by agent hooks. Unknown
    /// values fall back to `Default`.
    pub fn from_label(s: &str) -> Self {
        match s {
            "plan" => Self::Plan,
            "acceptEdits" => Self::AcceptEdits,
            "auto" => Self::Auto,
            "dontAsk" => Self::DontAsk,
            "bypassPermissions" => Self::BypassPermissions,
            "defer" => Self::Defer,
            _ => Self::Default,
        }
    }

    pub fn badge(&self) -> &str {
        match self {
            Self::Default => "",
            Self::Plan => "plan",
            Self::AcceptEdits => "edit",
            Self::Auto => "auto",
            Self::DontAsk => "dontAsk",
            Self::BypassPermissions => "!",
            Self::Defer => "defer",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentType {
    Claude,
    Codex,
    OpenCode,
    Devin,
    Copilot,
    Hermes,
    /// No recognized agent hook has claimed this pane. Used for plain
    /// tmux windows/panes surfaced by the `@sidebar_show_windows` toggle.
    Unknown,
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub window_id: String,
    pub window_name: String,
    pub window_active: bool,
    pub auto_rename: bool,
    pub panes: Vec<PaneInfo>,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_name: String,
    pub windows: Vec<WindowInfo>,
}

impl AgentType {
    /// Parse the agent label set by hooks. Returns `None` for unknown
    /// values so callers can skip non-agent panes.
    pub fn from_label(s: &str) -> Option<Self> {
        match s {
            CLAUDE_AGENT => Some(Self::Claude),
            CODEX_AGENT => Some(Self::Codex),
            OPENCODE_AGENT => Some(Self::OpenCode),
            DEVIN_AGENT => Some(Self::Devin),
            COPILOT_AGENT => Some(Self::Copilot),
            HERMES_AGENT => Some(Self::Hermes),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => CLAUDE_AGENT,
            Self::Codex => CODEX_AGENT,
            Self::OpenCode => OPENCODE_AGENT,
            Self::Devin => DEVIN_AGENT,
            Self::Copilot => COPILOT_AGENT,
            Self::Hermes => HERMES_AGENT,
            Self::Unknown => "unknown",
        }
    }

    pub fn label(&self) -> &str {
        self.as_str()
    }
}

impl PaneStatus {
    /// Parse the status label written by agent hooks. Unknown values
    /// map to `Unknown`.
    pub fn from_label(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "background" => Self::Background,
            "waiting" | "notification" => Self::Waiting,
            "idle" => Self::Idle,
            "error" => Self::Error,
            _ => Self::Unknown,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Running => "●",
            Self::Background => "◎",
            Self::Waiting => "◐",
            Self::Idle => "○",
            Self::Error => "✕",
            Self::Unknown => "·",
        }
    }

    /// `true` when the agent (or an owned background shell) is still
    /// doing work the user would expect to be timed or updated live.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Background | Self::Waiting)
    }
}

/// Coarse three-way classification of a pane's status, collapsing the six
/// `PaneStatus` variants (plus the `attention` flag) into the single
/// distinction a human skimming a status line or digest actually cares
/// about: does this pane need me right now, or can it keep going without
/// me? Mirrors the precedence `ColorTheme::status_color` already uses in
/// the TUI (attention overrides status), so a pane never renders
/// differently between the sidebar and the summary/digest CLI output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttentionBucket {
    /// Blocked on a permission prompt, a notification, or has errored —
    /// nothing will happen here until the user looks at it.
    NeedsYou,
    /// Actively running or driving a background shell.
    Working,
    /// Finished its last turn and is quietly waiting for the next prompt.
    Idle,
}

impl PaneInfo {
    /// Classify this pane for `summary`/`digest` output. See
    /// [`AttentionBucket`] for the precedence rules.
    pub fn attention_bucket(&self) -> AttentionBucket {
        if self.attention || self.status == PaneStatus::Error {
            AttentionBucket::NeedsYou
        } else if matches!(self.status, PaneStatus::Running | PaneStatus::Background) {
            AttentionBucket::Working
        } else {
            AttentionBucket::Idle
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pane(status: PaneStatus, attention: bool) -> PaneInfo {
        PaneInfo {
            pane_id: "%1".into(),
            pane_active: false,
            status,
            attention,
            agent: AgentType::Claude,
            path: "/tmp/project".into(),
            current_command: String::new(),
            prompt: String::new(),
            prompt_is_response: false,
            started_at: None,
            wait_reason: String::new(),
            permission_mode: PermissionMode::Default,
            subagents: vec![],
            pane_pid: None,
            worktree: WorktreeMetadata::default(),
            session_id: None,
            session_name: String::new(),
            sidebar_spawned: false,
            window_name: String::new(),
            bg_shell_cmd: None,
        }
    }

    #[test]
    fn attention_bucket_needs_you_when_attention_flag_set() {
        assert_eq!(
            test_pane(PaneStatus::Idle, true).attention_bucket(),
            AttentionBucket::NeedsYou
        );
    }

    #[test]
    fn attention_bucket_needs_you_on_error_even_without_attention_flag() {
        assert_eq!(
            test_pane(PaneStatus::Error, false).attention_bucket(),
            AttentionBucket::NeedsYou
        );
    }

    #[test]
    fn attention_bucket_working_for_running_and_background() {
        assert_eq!(
            test_pane(PaneStatus::Running, false).attention_bucket(),
            AttentionBucket::Working
        );
        assert_eq!(
            test_pane(PaneStatus::Background, false).attention_bucket(),
            AttentionBucket::Working
        );
    }

    #[test]
    fn attention_bucket_idle_for_idle_and_unknown() {
        assert_eq!(
            test_pane(PaneStatus::Idle, false).attention_bucket(),
            AttentionBucket::Idle
        );
        assert_eq!(
            test_pane(PaneStatus::Unknown, false).attention_bucket(),
            AttentionBucket::Idle
        );
    }

    #[test]
    fn attention_bucket_attention_flag_overrides_working_status() {
        // A pane can be `Running` but still flagged for attention (e.g. a
        // background shell that just emitted a notification); attention
        // must win so the bucket matches what the TUI would color it.
        assert_eq!(
            test_pane(PaneStatus::Running, true).attention_bucket(),
            AttentionBucket::NeedsYou
        );
    }

    #[test]
    fn pane_status_from_str_all_variants() {
        assert_eq!(PaneStatus::from_label("running"), PaneStatus::Running);
        assert_eq!(PaneStatus::from_label("background"), PaneStatus::Background);
        assert_eq!(PaneStatus::from_label("waiting"), PaneStatus::Waiting);
        assert_eq!(PaneStatus::from_label("notification"), PaneStatus::Waiting);
        assert_eq!(PaneStatus::from_label("idle"), PaneStatus::Idle);
        assert_eq!(PaneStatus::from_label("error"), PaneStatus::Error);
        assert_eq!(PaneStatus::from_label("anything"), PaneStatus::Unknown);
        assert_eq!(PaneStatus::from_label(""), PaneStatus::Unknown);
    }

    #[test]
    fn pane_status_icon_all_variants() {
        assert_eq!(PaneStatus::Running.icon(), "●");
        assert_eq!(PaneStatus::Background.icon(), "◎");
        assert_eq!(PaneStatus::Waiting.icon(), "◐");
        assert_eq!(PaneStatus::Idle.icon(), "○");
        assert_eq!(PaneStatus::Error.icon(), "✕");
        assert_eq!(PaneStatus::Unknown.icon(), "·");
    }

    #[test]
    fn agent_type_from_str_all() {
        assert_eq!(AgentType::from_label("claude"), Some(AgentType::Claude));
        assert_eq!(AgentType::from_label("codex"), Some(AgentType::Codex));
        assert_eq!(AgentType::from_label("opencode"), Some(AgentType::OpenCode));
        assert_eq!(AgentType::from_label("devin"), Some(AgentType::Devin));
        assert_eq!(AgentType::from_label("copilot"), Some(AgentType::Copilot));
        assert_eq!(AgentType::from_label("hermes"), Some(AgentType::Hermes));
        assert_eq!(AgentType::from_label("unknown"), None);
        assert_eq!(AgentType::from_label(""), None);
    }

    #[test]
    fn agent_type_label() {
        assert_eq!(AgentType::Claude.label(), "claude");
        assert_eq!(AgentType::Codex.label(), "codex");
        assert_eq!(AgentType::OpenCode.label(), "opencode");
        assert_eq!(AgentType::Devin.label(), "devin");
        assert_eq!(AgentType::Copilot.label(), "copilot");
        assert_eq!(AgentType::Hermes.label(), "hermes");
        assert_eq!(AgentType::Unknown.label(), "unknown");
    }

    #[test]
    fn agent_type_as_str_matches_constants() {
        assert_eq!(AgentType::Claude.as_str(), CLAUDE_AGENT);
        assert_eq!(AgentType::Codex.as_str(), CODEX_AGENT);
        assert_eq!(AgentType::OpenCode.as_str(), OPENCODE_AGENT);
        assert_eq!(AgentType::Devin.as_str(), DEVIN_AGENT);
        assert_eq!(AgentType::Copilot.as_str(), COPILOT_AGENT);
        assert_eq!(AgentType::Hermes.as_str(), HERMES_AGENT);
    }

    #[test]
    fn permission_mode_from_str_all() {
        assert_eq!(
            PermissionMode::from_label("default"),
            PermissionMode::Default
        );
        assert_eq!(PermissionMode::from_label("plan"), PermissionMode::Plan);
        assert_eq!(
            PermissionMode::from_label("acceptEdits"),
            PermissionMode::AcceptEdits
        );
        assert_eq!(PermissionMode::from_label("auto"), PermissionMode::Auto);
        assert_eq!(
            PermissionMode::from_label("dontAsk"),
            PermissionMode::DontAsk
        );
        assert_eq!(
            PermissionMode::from_label("bypassPermissions"),
            PermissionMode::BypassPermissions
        );
        assert_eq!(PermissionMode::from_label("defer"), PermissionMode::Defer);
        assert_eq!(PermissionMode::from_label(""), PermissionMode::Default);
        assert_eq!(
            PermissionMode::from_label("unknown"),
            PermissionMode::Default
        );
    }

    #[test]
    fn permission_mode_badge() {
        assert_eq!(PermissionMode::Default.badge(), "");
        assert_eq!(PermissionMode::Plan.badge(), "plan");
        assert_eq!(PermissionMode::AcceptEdits.badge(), "edit");
        assert_eq!(PermissionMode::Auto.badge(), "auto");
        assert_eq!(PermissionMode::DontAsk.badge(), "dontAsk");
        assert_eq!(PermissionMode::BypassPermissions.badge(), "!");
        assert_eq!(PermissionMode::Defer.badge(), "defer");
    }
}
