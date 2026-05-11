use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageManager {
    Npm,
    Pip,
    Brew,
    Scoop,
    Choco,
    Unknown,
}

impl PackageManager {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pip => "pip",
            Self::Brew => "brew",
            Self::Scoop => "scoop",
            Self::Choco => "choco",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "npm" => Self::Npm,
            "pip" => Self::Pip,
            "brew" => Self::Brew,
            "scoop" => Self::Scoop,
            "choco" => Self::Choco,
            _ => Self::Unknown,
        }
    }
}

pub fn detect_manager_from_path(path: &Path) -> PackageManager {
    let s = path.to_string_lossy().to_lowercase();
    let s = s.replace('\\', "/");

    if s.contains("/appdata/roaming/npm/")
        || s.contains("/.npm-global/bin/")
        || s.contains("/npm/bin/")
        || (s.contains("/node_modules/.bin/") && !s.contains("/local/"))
    {
        return PackageManager::Npm;
    }

    if s.contains("/opt/homebrew/")
        || s.contains("/usr/local/cellar/")
        || s.contains("/homebrew/cellar/")
    {
        return PackageManager::Brew;
    }

    if s.contains("/scoop/shims/") || s.contains("/scoop/apps/") {
        return PackageManager::Scoop;
    }

    if s.contains("/chocolatey/bin/") || s.contains("/choco/bin/") {
        return PackageManager::Choco;
    }

    if (s.contains("/scripts/") && (s.contains("python") || s.contains("/py")))
        || s.contains("/site-packages/")
        || s.contains("/.local/bin/")
    {
        return PackageManager::Pip;
    }

    PackageManager::Unknown
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalCliTool {
    pub id: String,
    pub detected_path: String,
    pub manager: PackageManager,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub last_checked: Option<String>,
    pub update_status: Option<String>,
    pub update_log: Option<String>,
}

impl LocalCliTool {
    pub fn new(id: &str, detected_path: &str, manager: PackageManager) -> Self {
        Self {
            id: id.to_string(),
            detected_path: detected_path.to_string(),
            manager,
            current_version: None,
            latest_version: None,
            update_available: false,
            last_checked: None,
            update_status: None,
            update_log: None,
        }
    }

    pub fn update_command(&self) -> Option<String> {
        match self.manager {
            PackageManager::Npm => Some(format!("npm install -g {}", self.id)),
            PackageManager::Pip => Some(format!("pip install --upgrade {}", self.id)),
            PackageManager::Brew => Some(format!("brew upgrade {}", self.id)),
            PackageManager::Scoop => Some(format!("scoop update {}", self.id)),
            PackageManager::Choco => Some(format!("choco upgrade {}", self.id)),
            PackageManager::Unknown => None,
        }
    }

    pub fn update_pty_args(&self) -> Option<(String, Vec<String>)> {
        let cmd = self.update_command()?;
        let parts: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        if parts.is_empty() {
            return None;
        }
        Some((parts[0].clone(), parts[1..].to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_manager_from_npm_path_windows() {
        let path = std::path::Path::new(
            r"C:\Users\user\AppData\Roaming\npm\bruce-doc-converter.cmd",
        );
        assert_eq!(detect_manager_from_path(path), PackageManager::Npm);
    }

    #[test]
    fn detect_manager_from_pip_path() {
        let path = std::path::Path::new(r"C:\Python311\Scripts\bruce-doc-converter.exe");
        assert_eq!(detect_manager_from_path(path), PackageManager::Pip);
    }

    #[test]
    fn detect_manager_from_brew_path() {
        let path = std::path::Path::new("/opt/homebrew/bin/pandoc");
        assert_eq!(detect_manager_from_path(path), PackageManager::Brew);
    }

    #[test]
    fn detect_manager_from_scoop_path() {
        let path = std::path::Path::new(r"C:\Users\user\scoop\shims\pandoc.exe");
        assert_eq!(detect_manager_from_path(path), PackageManager::Scoop);
    }

    #[test]
    fn detect_manager_unknown_for_system_path() {
        let path = std::path::Path::new("/usr/bin/git");
        assert_eq!(detect_manager_from_path(path), PackageManager::Unknown);
    }

    #[test]
    fn update_command_for_npm() {
        let tool = LocalCliTool::new("mmdc", "/opt/homebrew/bin/mmdc", PackageManager::Npm);
        assert_eq!(
            tool.update_command(),
            Some("npm install -g mmdc".to_string())
        );
    }

    #[test]
    fn update_command_is_none_for_unknown() {
        let tool = LocalCliTool::new("git", "/usr/bin/git", PackageManager::Unknown);
        assert_eq!(tool.update_command(), None);
    }
}
