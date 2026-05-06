use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 支持的编程工具类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentTool {
    /// 统一源目录 ~/.agents/skills
    Agents,
    /// Claude Code ~/.claude/skills
    ClaudeCode,
    /// Codex ~/.codex/skills
    Codex,
    /// Antigravity ~/.antigravity/skills
    Antigravity,
    /// OpenCode ~/.config/opencode/skills
    OpenCode,
}

impl AgentTool {
    /// 工具唯一标识符（用于序列化/数据库）
    pub fn id(&self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
            Self::OpenCode => "opencode",
        }
    }

    /// 用户友好显示名称
    pub fn label(&self) -> &'static str {
        match self {
            Self::Agents => "Universal (.agents)",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Antigravity => "Antigravity",
            Self::OpenCode => "OpenCode",
        }
    }

    /// 对应的 skill 目录（绝对路径）
    pub fn default_skills_dir(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let path = match self {
            Self::Agents => home.join(".agents").join("skills"),
            Self::ClaudeCode => home.join(".claude").join("skills"),
            Self::Codex => home.join(".codex").join("skills"),
            Self::Antigravity => home.join(".antigravity").join("skills"),
            Self::OpenCode => home.join(".config").join("opencode").join("skills"),
        };
        Some(path)
    }

    /// 从字符串 id 解析
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "agents" => Some(Self::Agents),
            "claude-code" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "antigravity" => Some(Self::Antigravity),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }

    /// 所有工具列表（顺序固定）
    pub fn all() -> Vec<Self> {
        vec![
            Self::Agents,
            Self::ClaudeCode,
            Self::Codex,
            Self::Antigravity,
            Self::OpenCode,
        ]
    }

    /// 检测本机已存在（父目录存在）的工具，仅用于 UI 默认勾选
    pub fn detect_present_tools() -> Vec<Self> {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return vec![Self::Agents],
        };

        let mut present = vec![Self::Agents];

        if home.join(".claude").exists() {
            present.push(Self::ClaudeCode);
        }
        if home.join(".codex").exists() {
            present.push(Self::Codex);
        }
        if home.join(".antigravity").exists() {
            present.push(Self::Antigravity);
        }
        if home.join(".config").join("opencode").exists() {
            present.push(Self::OpenCode);
        }

        present
    }
}

/// 工具信息（用于前端展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolInfo {
    pub id: String,
    pub label: String,
    pub path: Option<String>,
    pub present: bool,
    pub skill_count: usize,
}

impl AgentToolInfo {
    pub fn from_tool(tool: &AgentTool, skill_count: usize) -> Self {
        let path = tool.default_skills_dir();
        let present = path.as_ref().map(|p| p.parent().map(|pp| pp.exists()).unwrap_or(false)).unwrap_or(false);
        Self {
            id: tool.id().to_string(),
            label: tool.label().to_string(),
            path: path.map(|p| p.to_string_lossy().to_string()),
            present,
            skill_count,
        }
    }
}
