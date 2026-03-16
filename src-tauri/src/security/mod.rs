mod rules;
mod scanner;

pub use rules::SecurityRules;
pub use scanner::{ScanOptions, SecurityScanner};

use crate::models::security::*;
use anyhow::Result;

/// 安全检查器特征
pub trait SecurityChecker {
    fn scan_file(&self, content: &str, file_path: &str, locale: &str) -> Result<SecurityReport>;
    fn calculate_score(&self, issues: &[SecurityIssue]) -> i32;
}
