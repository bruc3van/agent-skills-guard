# 安全风险优先扫描器收敛计划

> 目标：把当前安全扫描器从“Cisco 本地扫描能力移植版”收敛为 Agent Skill 安装前的安全风险判断器。默认只强关注可执行风险、注入风险、外联风险、敏感数据风险；不可审计内容显式降信任；结构合规默认降噪；多余的深度解析和合规包袱逐步删除。

---

## 1. 背景与原则

当前 `src-tauri/src/security` 已经移植了较多 Cisco `skill_scanner` 本地能力，包括 strict structure、policy、SkillContext、archive extraction、file magic、analyzability、pipeline、homoglyph、cross-skill 等。但实测 `test/test-skills` 后，报告中 80%+ 是结构合规类噪音，例如：

- `STRUCTURE_DISALLOWED_SUBDIR`
- `STRUCTURE_DISALLOWED_EXTENSION`
- `STRUCTURE_BINARY_CONTENT`
- `MANIFEST_MISSING_LICENSE`
- 普通 `STRUCTURE_ORPHAN_SCRIPT`

这些 finding 会淹没真正关心的安全风险，并把正常 skill 打成 Critical。新的产品原则如下：

1. 默认主报告只强关注安全风险：命令执行、代码注入、Prompt Injection、网络外联、敏感数据读取/泄露、密钥、可疑 pipeline、可执行伪装。
2. 不可审计内容不深挖，但必须显式降信任：压缩包、PDF、Office、bytecode、大文件、未知二进制、无法完整扫描内容。
3. 结构合规默认降噪：保留为辅助信息或 strict 模式能力，不主导安全分和安装阻断。
4. 不追求 Cisco 完整能力覆盖，不默认做 PDF/Office 正文解析、bytecode 反编译、深层压缩包递归扫描、YARA 扩展。
5. 多余逻辑按“先下线默认路径，再删除模块/测试/fixture”的方式处理，避免一次性破坏安装流程。

---

## 2. 第三方计划的采纳与修正

第三方计划中可采纳的方向：

- 增加安全聚焦策略。
- 结构类 finding 从默认报告中降噪。
- 同形字检测对 Office XML 内部文件降噪。
- 让扫描结果聚焦注入、密钥、执行、Prompt Injection。

需要修正的点：

- 不建议用 `skip_categories: [PolicyViolation, SocialEngineering]` 粗暴禁用整类。`PolicyViolation` 中可能包含 allowed-tools 越权、不可审计内容等有安全意义的条目；`SocialEngineering` 中也可能包含 Prompt Injection 或误导性描述风险。
- `Finding.category` 当前是 `ThreatCategory`，不是 `Option`，计划中的 `if let Some(ref cat)` 不符合现有类型。
- 不建议只靠类别跳过 analyzer。应该先给 finding 增加更贴近产品展示的维度：`finding_kind = security | auditability | structure`，再基于 kind 控制展示、评分和阻断。
- `pipeline::analyze` 也应允许按 finding kind/规则级别处理，但默认 pipeline 结果大多属于 `security`，不能被结构降噪误伤。

---

## 3. 目标报告模型

新增三层 finding kind：

```rust
pub enum FindingKind {
    Security,
    Auditability,
    Structure,
}
```

含义：

| Kind | 含义 | 默认展示 | 默认计入安全分 | 默认阻断 |
|------|------|----------|----------------|----------|
| `Security` | 可执行风险、注入、外联、敏感数据、密钥、Prompt Injection | 主视图 | 是 | Critical/hard trigger 阻断 |
| `Auditability` | 不可审计或扫描覆盖不足 | 次级视图 | 轻度计入 | 默认不阻断，明确危险时升级 |
| `Structure` | 包装规范、目录/扩展名/许可证等合规问题 | 默认折叠 | 默认不计入或极低权重 | 默认不阻断 |

`ThreatCategory` 继续保留，用于安全语义；`FindingKind` 用于产品层展示、评分和阻断策略。不要用 `ThreatCategory` 直接代替 `FindingKind`。

---

## 4. 保留的默认能力

默认扫描必须保留并优先打磨：

### 4.1 `SKILL.md` 和 Skill 语义

- frontmatter 解析。
- `allowed-tools` 与实际脚本行为一致性。
- `compatibility` 中是否声明网络能力。
- 描述中明确恶意指令、Prompt Injection、越权能力膨胀。
- `SKILL.md` 中显式引用的文本、配置、模板、脚本。

### 4.2 脚本与可执行文本

重点扫描：

- `scripts/`
- `src/`
- `lib/`
- `bin/`
- `tools/`
- `commands/`
- 被 `SKILL.md` 引用的本地脚本

检测类型：

- `curl | sh`、`iex`、`eval`、`exec`、`subprocess`
- 敏感文件读取：`.env`、SSH key、token、browser profile、credential store
- 网络外联：`requests.post`、`fetch`、`curl -X POST`、socket
- pipeline 组合攻击：download -> chmod -> execute，read secret -> encode -> exfil
- 混淆执行：base64 decode 后执行、hex blob 后执行、动态 import/getattr

### 4.3 文本与引用资源

默认扫描可读文本：

- `.md`
- `.txt`
- `.json`
- `.yaml/.yml`
- `.toml`
- `.html`
- `.svg`
- `.xml`
- `.env`
- `.ini/.cfg`

重点检测：

- Prompt Injection
- suspicious URL
- secret
- command/pipeline 片段
- homoglyph/zero-width，但需要对生成型 Office XML 降噪

### 4.4 明确安装风险

继续阻断：

- symlink
- 路径穿越
- 可执行内容伪装成文本或图片
- 高置信度 hard trigger
- 明确恶意 pipeline
- 高置信度密钥泄露

---

## 5. 默认下线或删除的能力

### 5.1 Archive 深度解析

默认不递归解压并扫描压缩包内容。

保留轻量 inventory：

- 识别归档存在。
- 检查路径穿越、symlink、zip bomb、高压缩比。
- 检查 entry 文件名和扩展名是否包含可执行内容：
  - `.exe`
  - `.dll`
  - `.so`
  - `.dylib`
  - `.sh`
  - `.bash`
  - `.ps1`
  - `.bat`
  - `.cmd`
  - `.py`
  - `.js`
  - `.jar`
  - `.pyc`
- 发现嵌套归档只作为 `Auditability`，不递归挖。

删除/下线方向：

- `scanner.rs` 中解压后逐个读取并调用 `scan_text_content` 的默认路径应删除或放入 strict-only。
- `archive_extractor.rs` 可以瘦身为 `archive_inventory.rs`，只读取 central directory / tar header，不默认输出临时解压目录。
- `MAX_EXTRACTED_FILE_BYTES` 默认路径不再需要。

### 5.2 PDF / Office 正文解析

默认不解析正文。

保留轻量检查：

- magic mismatch。
- 文件大小。
- Office zip entry 中的 VBA / OLE 指示：
  - `vbaProject.bin`
  - `embeddings/`
  - `oleObject`
- 被 `SKILL.md` 显式要求读取时，输出 `Auditability` 或 indirect prompt injection 提示。

删除/下线方向：

- 不引入 PDF 文本抽取。
- 不扫描 docx 内部 `word/fontTable.xml`、`word/theme/theme1.xml` 的普通文本规则和 homoglyph。
- Office 内部 XML 默认不进入主 security findings，除非命中宏/OLE/外链等明确危险信号。

### 5.3 Bytecode 反编译

默认不反编译 `.pyc/.class`。

处理方式：

- 作为不可审计可执行内容，输出 `Auditability`。
- 如果位于脚本执行路径或被 `SKILL.md` 引用，升级为 `Security` 中高风险。

删除/下线方向：

- 不移植 Cisco `bytecode_analyzer.py`。
- 删除与 bytecode 深度分析相关的计划、fixture 和 parity 目标。

### 5.4 结构合规默认降噪

默认不让以下规则进入主风险评分：

- `STRUCTURE_DISALLOWED_SUBDIR`
- `STRUCTURE_DISALLOWED_EXTENSION`
- `STRUCTURE_BINARY_CONTENT`
- `STRUCTURE_NAME_DIR_MISMATCH`
- `MANIFEST_MISSING_LICENSE`
- `STRUCTURE_ORPHAN_SCRIPT`
- `TRIGGER_DESCRIPTION_TOO_SHORT`
- `TRIGGER_KEYWORD_BAITING`
- `TRIGGER_VAGUE_DESCRIPTION`
- `LOW_ANALYZABILITY`

这些规则保留为 `Structure` 或 `Auditability`，默认折叠展示。strict 模式或“发布规范检查模式”可重新启用显著展示。

---

## 6. 策略设计

### 6.1 默认策略改为 security-focused

建议将 `default.yaml` 的语义调整为 security-focused，而不是新增一个只有测试使用的策略。原因：用户实际使用客户端时默认就应该看到降噪后的安全风险判断。

保留 `strict.yaml` 给完整合规检查。

保留 `permissive.yaml` 给更宽松的本地调试。

### 6.2 新增 policy 字段

在 `ScanPolicy` 中新增：

```rust
#[serde(default = "default_score_kinds")]
pub score_kinds: HashSet<String>;

#[serde(default)]
pub strict_structure_enabled: bool;

#[serde(default)]
pub archive_deep_scan_enabled: bool;
```

建议默认值：

```yaml
score_kinds:
  - Security
  - Auditability

strict_structure_enabled: false
archive_deep_scan_enabled: false
```

说明：

- 不用 `skip_categories` 作为主机制。
- `disabled_rules` 继续用于精确禁用规则。
- 不保留 `disabled_kinds` / `visible_kinds` 这类未落地的展示开关；展示层可根据 `finding_kind` 分组，扫描器只负责产生真实风险语义。
- `strict_structure_enabled=false` 时，结构 analyzer 不运行，结构类 context finding 默认不输出；symlink 检查保留在主遍历中。
- `archive_deep_scan_enabled=false` 时，不解压归档，只输出 `ARCHIVE_FILE_DETECTED` 和文件魔数风险。

### 6.3 规则到 FindingKind 的映射

新增 `finding_kind_for_rule(rule_id, threat_category, analyzer)`：

Security：

- `CURL_PIPE_SH`
- `REVERSE_SHELL`
- `COMMAND_INJECTION_*`
- `PROMPT_INJECTION_*`
- `SECRET_*`
- `API_KEY`
- `PRIVATE_KEY`
- `DATA_EXFIL_*`
- `PIPELINE_*`
- `TAINT_*`
- `PICKLE_LOAD`
- `SUBPROCESS_CALL`
- `HOMOGLYPH_ATTACK`（非 Office 内部 XML）
- `FILE_MAGIC_MISMATCH`（文本/图片伪装可执行、归档、Office）

Auditability：

- `ARCHIVE_FILE_DETECTED`
- `ARCHIVE_NESTED_TOO_DEEP`
- `ARCHIVE_CONTAINS_EXECUTABLE`（若只是 entry 名称，High auditability；若执行路径引用则升 Security）
- `UNANALYZABLE_BINARY`
- `OVERSIZED_FILE`
- `LOW_ANALYZABILITY`
- `STRUCTURE_BINARY_CONTENT`
- 普通 PDF/Office/bytecode 存在

Structure：

- `STRUCTURE_DISALLOWED_SUBDIR`
- `STRUCTURE_DISALLOWED_EXTENSION`
- `STRUCTURE_NAME_DIR_MISMATCH`
- `MANIFEST_MISSING_LICENSE`
- `STRUCTURE_ORPHAN_SCRIPT`
- `TRIGGER_DESCRIPTION_TOO_SHORT`
- `TRIGGER_KEYWORD_BAITING`
- `TRIGGER_VAGUE_DESCRIPTION`

---

## 7. 代码改造计划

### P0：数据模型与分类映射

目标：先不删检测器，只让报告有能力区分安全风险、不可审计、结构噪音。

修改文件：

- `src-tauri/src/models/security.rs`
- `src-tauri/src/security/scanner.rs`
- `src-tauri/src/security/policy.rs`

内容：

1. 新增 `FindingKind`。
2. `FindingMetadata` 增加 `finding_kind: Option<String>`。
3. `SecurityIssue` 增加 `finding_kind: Option<String>`。
4. `issue_from_match`、`issue_from_finding` 填充 kind。
5. 新增统一映射函数，不在每个 analyzer 内散落判断。

验收：

- 现有测试通过。
- 每个 finding 都能映射到 `Security/Auditability/Structure`。
- 旧报告反序列化兼容，新字段可缺省。

### P1：评分降噪

目标：结构合规不再把 skill 打成 Critical。

修改文件：

- `src-tauri/src/security/scanner.rs`
- `src-tauri/tests/scan_test_skills.rs`（如已有本地改动，实施时先合并用户改动）

内容：

1. `calculate_score_from_issues` 只对 `score_kinds` 计分。
2. `Structure` 默认不计分。
3. `Auditability` 低权重计分，明确危险可升级为 `Security`。
4. hard block 只由明确 `Security` hard trigger 或 symlink/path traversal 触发。
5. `generate_recommendations` 优先基于 `Security` issues。

验收：

- 只有结构问题的 skill 不应低于 70。
- 有 `CURL_PIPE_SH` / `PROMPT_INJECTION_*` / `SECRET_*` 仍明显降分或阻断。
- `test/test-skills` 主报告中结构类不再占主要输出。

### P2：结构校验默认下线

目标：默认不运行完整 strict structure。

修改文件：

- `src-tauri/src/security/scanner.rs`
- `src-tauri/src/security/strict_structure.rs`
- `src-tauri/resources/security/policies/default.yaml`
- `src-tauri/resources/security/policies/strict.yaml`

内容：

1. `default.yaml` 设置 `strict_structure_enabled: false`。
2. `strict.yaml` 设置 `strict_structure_enabled: true`。
3. `scan_directory_with_options` 中只有 `strict_structure_enabled=true` 才运行 `strict_structure::validate`。
4. symlink 检查保留在 scanner 主遍历中，不依赖 strict structure。
5. missing license、disallowed subdir、disallowed extension、name mismatch 等默认不输出。

删除候选：

- 如果后续确认 strict 模式不是产品需要，可删除大部分 `strict_structure.rs` 合规规则，只保留 symlink、隐藏可执行、路径异常。

验收：

- 默认扫描不再输出 `STRUCTURE_DISALLOWED_SUBDIR`、`STRUCTURE_DISALLOWED_EXTENSION`、`MANIFEST_MISSING_LICENSE`。
- strict policy 下仍可输出结构问题。

### P3：Archive 改成轻量 inventory

目标：默认不深度解压扫描。

修改文件：

- `src-tauri/src/security/archive_extractor.rs`
- `src-tauri/src/security/scanner.rs`
- `src-tauri/tests/cisco_parity.rs`
- `src-tauri/tests/fixtures/security/cisco_parity/manifest.yaml`

内容：

1. 新增或重命名为 `archive_inventory` API：
   - 输入 archive path 和 policy。
   - 输出 archive findings 和 entry summary。
   - 不返回 `temp_dir`。
2. 删除默认路径中对 `extraction.extracted_files` 的逐文件扫描。
3. 归档存在输出 `Auditability`。
4. 路径穿越、symlink、zip bomb 输出 `Security` 或 high auditability，并可阻断。
5. entry 中有可执行扩展名输出 `Auditability`；若 entry 被 `SKILL.md` 引用或执行路径命中，升级 `Security`。
6. cisco parity 中关于“zip 内容脚本必须被深度扫描”的预期删除或改为 strict-only。

删除候选：

- 默认扫描路径删除 `MAX_EXTRACTED_FILE_BYTES`。
- 删除“把 archive 内文本作为普通文本继续跑所有规则”的逻辑。
- 删除深层 archive parity fixtures，或移动到 strict-only 测试。

验收：

- 普通 zip 不阻断。
- path traversal zip 阻断。
- zip 内 `.exe/.sh/.py` 提示不可审计或可执行风险。
- zip 内普通文本不进入主 security 规则扫描。

### P4：Office/PDF/bytecode 降信任，不深挖

目标：删除深度文档/bytecode 分析负担。

修改文件：

- `src-tauri/src/security/scanner.rs`
- `src-tauri/src/security/file_magic.rs`
- `src-tauri/src/security/analyzability.rs`
- `src-tauri/src/security/homoglyph.rs`（尽量不改内部，只改调用侧）

内容：

1. Office 内部 XML 不跑 homoglyph。
2. Office/PDF 不解析正文。
3. Office 只检查 macro/OLE entry。
4. `.pyc/.class/.jar` 输出不可审计可执行内容，不反编译。
5. `LOW_ANALYZABILITY` 默认从主评分/主报告移出。

删除候选：

- 删除 P4 YARA/PDF/Office/bytecode 深度移植计划。
- 删除无产品价值的 Office XML 文本扫描路径。

验收：

- docx 内部 `word/fontTable.xml` 不再产生 `HOMOGLYPH_ATTACK`。
- 普通 PDF/Office 只出 auditability 提示或不出主风险。
- 宏/OLE 仍能检出。

### P5：前端/报告展示适配

目标：用户默认看到真正安全风险。

修改文件：

- 前端报告组件（实施时用 `rg "security_issues|securityReport|rule_id"` 定位）
- `src-tauri/src/models/security.rs`
- `src-tauri/src/services/database.rs`

内容：

1. 报告增加 kind 计数：
   - security_count
   - auditability_count
   - structure_count
2. 默认展开 `Security`。
3. `Auditability` 次级显示。
4. `Structure` 默认折叠或只在 strict 模式显示。
5. 数据库旧报告兼容缺省 kind。

验收：

- test-skills 扫描结果主视图不再被结构类淹没。
- 用户仍可展开查看结构/不可审计问题。

### P6：删除 Cisco 完整移植包袱

目标：把计划和测试目标从“完整 parity”改成“产品相关安全风险覆盖”。

修改文件：

- `docs/security-scanner-improvement-plan.md`
- `docs/security-scanner-fix-plan.md`
- `src-tauri/tests/cisco_parity.rs`
- `src-tauri/tests/fixtures/security/cisco_parity/manifest.yaml`

内容：

1. 将 Cisco parity 定位为参考用例来源，不作为完整覆盖目标。
2. 删除或降级以下 parity 目标：
   - 深层 archive extraction。
   - bytecode analyzer。
   - PDF 正文解析。
   - Office XML 文本扫描。
   - 纯结构合规触发 Critical。
3. 保留以下 parity 目标：
   - command injection。
   - prompt injection。
   - secrets。
   - data exfiltration。
   - pipeline。
   - file magic executable disguise。
   - path traversal/symlink。
4. 对 `scan_test_skills` 输出建立降噪基线：
   - 主 security findings 数量。
   - structure findings 数量。
   - top rules。
   - score 分布。

验收：

- 文档不再承诺“完整移植 Cisco 本地扫描逻辑”。
- 测试反映 Agent-skills-guard 的产品目标。

---

## 8. 推荐实施顺序

1. P0 数据模型与 kind 映射。
2. P1 评分降噪。
3. P2 结构校验默认下线。
4. P4 Office/PDF/bytecode 降信任。
5. P3 Archive inventory 化。
6. P5 前端展示。
7. P6 删除过时 parity 和旧计划内容。

原因：

- P0/P1 先解决最痛的噪音和评分问题，风险最低。
- P2 立刻减少 80%+ 结构噪音。
- P3/P4 涉及行为删减，应在报告和评分模型稳定后做。
- P6 最后做，避免测试先删导致实现没有保护。

---

## 9. 验证矩阵

### 9.1 默认安全聚焦扫描

运行：

```powershell
cd src-tauri
cargo test security::scanner -- --nocapture
cargo test --test cisco_parity -- --nocapture
cargo test --test scan_test_skills -- --nocapture
```

期望：

- `CURL_PIPE_SH`、`PROMPT_INJECTION_*`、`SECRET_*`、`DATA_EXFIL_*`、`PIPELINE_*` 仍检出。
- 默认主报告不再大量输出 `STRUCTURE_*`。
- docx 内部 XML 不再产生普通 `HOMOGLYPH_ATTACK`。
- 普通 PDF/Office/zip 不 hard block。
- path traversal / symlink / executable disguise 仍 hard block 或高风险。

### 9.2 Strict 模式

新增/保留 strict 测试：

- strict policy 下 `STRUCTURE_DISALLOWED_SUBDIR` 可出现。
- strict policy 下 `MANIFEST_MISSING_LICENSE` 可出现。
- strict policy 下 archive deep scan 如保留，可以单独验证。

### 9.3 降噪指标

对 `test/test-skills` 记录：

- total findings。
- security findings。
- auditability findings。
- structure findings。
- top 20 rule ids。
- score 分布。

验收目标：

- 默认主视图中 `Structure` 不超过主展示 finding 的 20%。
- 只有结构问题的 skill 不进入 Critical。
- 真正安全风险不会因降噪消失。

---

## 10. 明确不做

- 不引入 LLM、远程 API、VirusTotal。
- 不追求 Cisco 完整 parity。
- 不默认解析 PDF 正文。
- 不默认解析 Office 正文。
- 不反编译 bytecode。
- 不默认递归解压扫描压缩包内容。
- 不让结构合规主导安装安全结论。

---

## 11. 最终产品表述

扫描器定位：

> Agent Skill 安装前安全风险扫描器。默认聚焦可执行风险、注入攻击、网络外联和敏感数据风险；对不可审计内容显式降低信任；结构合规作为辅助信息，不主导安全结论。
