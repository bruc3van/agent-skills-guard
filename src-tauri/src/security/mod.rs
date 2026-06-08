pub mod analyzability;
pub mod archive_extractor;
pub mod asset_checks;
pub mod consistency_checker;
pub mod cross_skill;
pub mod file_magic;
pub mod finding_builder;
pub mod homoglyph;
pub mod pipeline;
pub mod policy;
pub mod referenced_files;
mod rules;
mod scanner;
pub mod secret_masking;
pub mod skill_context;
pub mod strict_structure;

pub use policy::ScanPolicy;
pub use scanner::{ScanOptions, SecurityScanner};

/// 常见大目录（依赖/构建产物/VCS 缓存），默认不深入扫描。
///
/// scanner.rs 和 skill_context.rs 共用此常量，确保目录遍历行为一致。
pub const SKIP_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
];
