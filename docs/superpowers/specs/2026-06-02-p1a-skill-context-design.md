# P1a: SkillContext 构建、结构扫描与流程兼容 — 设计文档

> 日期：2026-06-02
> 状态：已批准
> 关联：[安全扫描器升级计划](../../security-scanner-improvement-plan.md) 第 4.3 节、第 5 节 P1a 里程碑

## 1. 目标

建立 Skill 语义解析基础，补齐结构扫描能力，确保现有流程不退化。

### 验收标准

1. `scan_file(SKILL.md)` 以 `ScanMode::SingleFile` 运行，不因缺少目录上下文产生 `MISSING_SKILL_MD`、引用缺失、orphan scripts 等结构类误报。
2. `scan_directory_with_options` 以 `ScanMode::Directory` 运行，结构扫描覆盖全部白名单校验项。
3. 引用文件提取覆盖所有指定模式，且能区分 stdlib/第三方与本地模块。
4. strict structure 校验覆盖允许子目录、允许扩展名、UTF-8、NUL/binary、frontmatter、name/description/compatibility。
5. 报告不泄露完整 secret。
6. 现有 `enforce_installable_report` 依赖字段（`blocked`、`hard_trigger_issues`、`partial_scan`、`skipped_files`）继续正确填充。

## 2. 模块组织

采用**扁平模块结构**（方案 A），每个模块一个职责：

```
src-tauri/src/security/
  mod.rs
  scanner.rs           # 重构为编排层
  skill_context.rs     # SkillContext + ScanMode + SkillManifest + SkillFile
  strict_structure.rs  # 结构校验
  referenced_files.rs  # 引用文件提取
  secret_masking.rs    # Secret 脱敏
  policy.rs            # 现有
  rules/               # 现有
```

## 3. 数据模型

### 3.1 ScanMode

```rust
pub enum ScanMode {
    SingleFile,  // scan_file 使用：只有内容级规则运行
    Directory,   // scan_directory_with_options 使用：全部 analyzer 运行
}
```

各 analyzer 入口首先检查 `scan_mode`：`SingleFile` 模式下跳过需要目录上下文的检查。

### 3.2 SkillContext

```rust
pub struct SkillContext {
    pub scan_mode: ScanMode,
    pub skill_dir: Option<PathBuf>,
    pub skill_md_path: Option<PathBuf>,
    pub manifest: Option<SkillManifest>,
    pub instruction_body: Option<String>,
    pub files: Vec<SkillFile>,
    pub referenced_files: Vec<String>,
    pub script_files: Vec<String>,
    pub asset_files: Vec<String>,
    pub scan_policy: ScanPolicy,
}
```

- `SingleFile` 模式下 `skill_dir`/`files`/`referenced_files` 为 `None` 或空
- `SkillManifest` 从 SKILL.md 的 YAML frontmatter 解析

### 3.3 SkillManifest

```rust
pub struct SkillManifest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub compatibility: Option<String>,
    pub metadata: Option<serde_yaml::Value>,
}
```

### 3.4 SkillFile

```rust
pub struct SkillFile {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub file_type: SkillFileType,
    pub size_bytes: u64,
    pub is_binary: bool,
    pub is_hidden: bool,
}

pub enum SkillFileType {
    Markdown,
    Script,
    Config,
    Asset,
    Binary,
    Unknown,
}
```

## 4. Frontmatter 解析

在 `skill_context.rs` 内实现：

1. 查找 SKILL.md（任意大小写）
2. 提取 `---` 分隔的 YAML frontmatter
3. 解析为 `SkillManifest`，缺失字段用 `None` 填充
4. 提取 frontmatter 之后的内容作为 `instruction_body`
5. 解析失败不阻断扫描，产生 `FRONTMATTER_PARSE_ERROR` finding（Medium）

## 5. 结构校验

在 `strict_structure.rs` 实现 5 步流水线：

1. 目录存在性检查
2. 遍历条目，检查白名单
3. 编码校验（UTF-8、NUL/binary）
4. SKILL.md 存在性检查（任意大小写）
5. frontmatter 校验（name/description/compatibility）

### Finding ID 映射

| 检查项 | Finding ID | 严重度 |
|--------|-----------|--------|
| 符号链接 | `STRUCTURE_SYMLINK` | Critical |
| 隐藏文件/目录 | `STRUCTURE_HIDDEN_FILE` | Medium |
| 不允许的子目录 | `STRUCTURE_DISALLOWED_SUBDIR` | Medium |
| 不允许的扩展名 | `STRUCTURE_DISALLOWED_EXTENSION` | Medium |
| 二进制/NUL 内容 | `STRUCTURE_BINARY_CONTENT` | Low |
| 非 UTF-8 编码 | `STRUCTURE_NON_UTF8` | Low |
| SKILL.md 缺失 | `STRUCTURE_MISSING_SKILL_MD` | High |
| frontmatter 解析失败 | `FRONTMATTER_PARSE_ERROR` | Medium |
| name 缺失/格式错误 | `STRUCTURE_INVALID_NAME` | Medium |
| description 缺失/过短/过长 | `STRUCTURE_INVALID_DESCRIPTION` | Medium |

白名单来自 `ScanPolicy.strict_structure`：
- `allowed_extensions`: `.md`, `.py`, `.sh`, `.json`, `.yaml`, `.yml`, `.txt`, `.js`, `.ts`, `.html`, `.css`, `.svg`, `.xml`, `.xsd`, `.toml`, `.cfg`, `.ini`, `.env`, `.gitignore`, `.gitattributes`, `.editorconfig`, `.prettierrc`, `.eslintrc`
- `allowed_subdirs`: `scripts`, `references`, `assets`, `templates`, `data`, `config`, `src`, `lib`
- name 格式：`^[a-z0-9](?:[a-z0-9]|-(?!-))*[a-z0-9]$`（小写字母数字，单连字符，长度 2-64）
- description：非空，长度 10-1024

`ScanMode::SingleFile` 下跳过所有结构检查。

## 6. 引用文件提取

在 `referenced_files.rs` 实现 6 种提取模式：

| 模式 | 正则 | 说明 |
|------|------|------|
| Markdown 链接 | `\[.*?\]\(([^)]+)\)` | 排除 URL、锚点、绝对路径和路径穿越 |
| 自然语言引用 | `(see\|refer to\|check\|read)\s+`[\"']?(\S+\.\w+)` | 反引号/引号包裹的文件路径 |
| 执行型引用 | `(run\|execute\|invoke)\s+scripts/\S+` | scripts/ 下的脚本 |
| `@reference:` 指令 | `@reference:\s*(.+)` | 专用指令 |
| `include/import/load:` | `(include\|import\|load):\s*(.+)` | 配置类引用 |
| Python import | `^(?:from\s+(\S+)\s+)?import\s+(\S+)` | 区分 stdlib/第三方/本地 |

### Python 模块分类

- 内置 stdlib 模块列表：硬编码在 `referenced_files.rs` 中的 `const STDLIB_MODULES: &[&str]`（约 200 个，覆盖 Python 3.9+ 标准库）
- 常见第三方包列表：硬编码在 `referenced_files.rs` 中的 `const KNOWN_THIRD_PARTY: &[&str]`（约 100 个：`requests`, `flask`, `numpy`, `pandas`, `django`, `fastapi` 等）
- 不在以上列表中的 import 语句，如果对应目录中有同名 `.py` 文件，视为本地模块

## 7. Secret 脱敏

在 `secret_masking.rs` 实现：

| Secret 类型 | 脱敏规则 |
|------------|---------|
| AWS Key (`AKIA...`) | 保留 `AKIA` 前缀 + 后 4 位 |
| GitHub Token (`ghp_...`) | 保留 `ghp_` 前缀 + 后 4 位 |
| 私钥 | 只保留 `BEGIN ... PRIVATE KEY` 类型说明 |
| JWT Token | 保留 `eyJ` 前缀 + 类型段 |
| DB 连接串 | 保留协议前缀，隐藏凭证 |
| 通用 token | 保留前 4 位 |

脱敏在 finding 生成后、写入报告前执行。原始完整 secret 不进入日志、数据库和 UI。

## 8. Scanner 集成

### 8.1 scan_file 重构

```rust
pub fn scan_file(&self, content: &str, file_path: &str, locale: &str) -> Result<SecurityReport> {
    // 1. 构建 SkillContext（ScanMode::SingleFile）
    let ctx = SkillContext::for_single_file(content, file_path, policy);

    // 2. 运行内容级规则（现有 pattern matching）
    let matches = self.run_content_rules(&ctx);

    // 3. Secret 脱敏
    let findings = self.mask_secrets(matches);

    // 4. 生成 SecurityReport（保持现有字段兼容）
    self.build_report(findings, &ctx)
}
```

### 8.2 scan_directory_with_options 重构

```rust
pub fn scan_directory_with_options(...) -> Result<SecurityReport> {
    // 1. 构建 SkillContext（ScanMode::Directory）
    let ctx = SkillContext::for_directory(dir_path, policy)?;

    // 2. 运行内容级规则（现有 pattern matching，遍历文件）
    let matches = self.run_content_rules_for_directory(&ctx);

    // 3. 运行结构校验（仅 Directory 模式）
    let structure_findings = strict_structure::validate(&ctx);

    // 4. 运行引用文件完整性检查（仅 Directory 模式）
    let ref_findings = referenced_files::check_integrity(&ctx);

    // 5. 运行 hidden files / orphan scripts（仅 Directory 模式）
    let hidden_findings = self.check_hidden_and_orphans(&ctx);

    // 6. 合并所有 findings
    let all_findings = merge(matches, structure_findings, ref_findings, hidden_findings);

    // 7. Secret 脱敏
    let findings = self.mask_secrets(all_findings);

    // 8. 生成 SecurityReport（保持现有字段兼容）
    self.build_report(findings, &ctx)
}
```

### 8.3 兼容性保证

- `SecurityReport.blocked`：hard_trigger 规则命中时设为 true
- `SecurityReport.hard_trigger_issues`：继续填充 i18n 格式的硬触发描述
- `SecurityReport.partial_scan`：文件数超限、截断、不可读时设为 true
- `SecurityReport.skipped_files`：二进制/不可读文件继续记录
- 现有 `calculate_score_weighted` 算法不变
- 现有 `generate_recommendations` 逻辑不变

## 9. 测试策略

| 测试类别 | 覆盖内容 |
|---------|---------|
| Frontmatter 测试 | 正常解析、缺失字段、解析失败、多行 YAML、无 frontmatter |
| 结构校验测试 | symlink、hidden file、disallowed extension/subdir、binary content、non-UTF8、missing SKILL.md |
| 引用提取测试 | Markdown 链接、自然语言引用、执行引用、`@reference:`、`include/import/load:`、Python import（stdlib vs 本地 vs 第三方） |
| Secret 脱敏测试 | AWS key、GitHub token、私钥、JWT、DB 连接串、通用 token |
| 流程兼容测试 | scan_file 不产生结构误报、scan_directory 覆盖全部校验、现有字段正确填充 |
| 回归测试 | 现有 31 个 scanner 单元测试全部通过 |

## 10. 实现顺序

1. `skill_context.rs` — SkillContext、ScanMode、SkillManifest、SkillFile、frontmatter 解析
2. `strict_structure.rs` — 结构校验流水线
3. `referenced_files.rs` — 引用文件提取
4. `secret_masking.rs` — Secret 脱敏
5. `scanner.rs` 重构 — 编排层改造
6. 测试 — 全部新测试 + 回归测试
